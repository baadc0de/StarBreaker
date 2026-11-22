//! Decode the LightStyles table in the installed renderer's tokenized Light.cfx shader.
use std::collections::{BTreeMap, HashMap};

use serde::Serialize;
use starbreaker_p4k::MappedP4k;

const HEADER_SIZE: usize = 28;
// FXB1 built-in token IDs. The shader's identifier and non-built-in numbers
// are stored in its trailing name table.
const LIGHT_STYLE: u32 = 262;
const MAP: u32 = 417;
const TIME_INCREMENT: u32 = 422;
const OPEN: u32 = 29;
const CLOSE: u32 = 30;
const SEPARATOR: u32 = 15; // || separates the RGB or grayscale keys
const DOT: u32 = 34;
const ASSIGN: u32 = 41;
const TERMINATOR: u32 = 36;

#[derive(Serialize)]
pub(crate) struct LightStyle {
    // Constant styles can omit the rate.
    samples_per_second: Option<f32>,
    samples: Vec<[f32; 3]>,
}

pub(crate) fn load(p4k: &MappedP4k) -> Option<BTreeMap<u32, LightStyle>> {
    for backend in ["Vulkan", "D3D11"] {
        let archive = format!(r"Engine\ShaderCache_{backend}.pak");
        let path = format!("Shaders/CFXIB/{backend}/Light.cfxb");
        match p4k.read_stored_archive_file(&archive, &path) {
            Ok(shader) => match decode(&shader) {
                Some(styles) => return Some(styles),
                None => log::warn!("Unrecognized light style table in {archive}/{path}"),
            },
            Err(err) => log::warn!("Could not read {archive}/{path}: {err}"),
        }
    }
    None
}

fn word(bytes: &[u8]) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(..4)?.try_into().ok()?))
}

fn decode(bytes: &[u8]) -> Option<BTreeMap<u32, LightStyle>> {
    if !bytes.starts_with(b"FXB1") {
        return None;
    }
    let names_offset = word(bytes.get(12..)?)? as usize;
    let token_bytes = bytes.get(HEADER_SIZE..names_offset)?;
    if token_bytes.len() % 4 != 0 {
        return None;
    }
    let tokens: Vec<_> = token_bytes.chunks_exact(4).filter_map(word).collect();
    let mut names = HashMap::new();
    let mut remaining = bytes.get(names_offset..)?;
    while !remaining.is_empty() {
        let id = word(remaining)?;
        let end = remaining.get(4..)?.iter().position(|&byte| byte == 0)? + 4;
        if let Ok(name) = std::str::from_utf8(&remaining[4..end]) {
            names.insert(id, name);
        }
        remaining = &remaining[end + 1..];
    }
    let first = tokens
        .iter()
        .position(|token| names.get(token) == Some(&"LightStyles"))?;
    parse_table(tokens.get(first + 1..)?, &names)
}

fn number(token: u32, names: &HashMap<u32, &str>) -> Option<f32> {
    match token {
        432..=456 => Some((token - 432) as f32),
        _ => names.get(&token)?.parse().ok(),
    }
}

fn sample(mut tokens: &[u32], names: &HashMap<u32, &str>) -> Option<[f32; 3]> {
    let mut values = [0.; 3];
    let mut count = 0;
    while let Some((&token, rest)) = tokens.split_first() {
        let value = if token == DOT || rest.first() == Some(&DOT) {
            let (digits, rest) = if token == DOT { rest } else { &rest[1..] }.split_first()?;
            tokens = rest;
            let whole = if token == DOT {
                0.
            } else {
                number(token, names)?
            };
            let digits = match digits {
                432..=456 => (digits - 432).to_string(),
                _ => names.get(digits)?.to_string(),
            };
            whole + format!("0.{digits}").parse::<f32>().ok()?
        } else {
            tokens = rest;
            number(token, names)?
        };
        *values.get_mut(count)? = value;
        count += 1;
    }
    match count {
        1 => Some([values[0]; 3]),
        3 => Some(values),
        _ => None,
    }
}

fn parse_table(
    mut tokens: &[u32],
    names: &HashMap<u32, &str>,
) -> Option<BTreeMap<u32, LightStyle>> {
    let [OPEN, rest @ ..] = tokens else {
        return None;
    };
    tokens = rest;
    let mut styles = BTreeMap::new();
    while let [LIGHT_STYLE, id, OPEN, rest @ ..] = tokens {
        let id = number(*id, names)? as u32;
        let (samples_per_second, rest) = match rest {
            [TIME_INCREMENT, ASSIGN, rate, TERMINATOR, rest @ ..] => {
                (Some(number(*rate, names)?), rest)
            }
            _ => (None, rest),
        };
        let [MAP, ASSIGN, OPEN, rest @ ..] = rest else {
            return None;
        };
        let end = rest.iter().position(|&token| token == CLOSE)?;
        let samples: Vec<_> = rest[..end]
            .split(|&token| token == SEPARATOR)
            .filter(|key| !key.is_empty())
            .map(|key| sample(key, names))
            .collect::<Option<_>>()?;
        let [CLOSE, CLOSE, rest @ ..] = &rest[end..] else {
            return None;
        };
        tokens = rest;
        if samples.is_empty()
            || (samples.len() > 1
                && !samples_per_second.is_some_and(|rate| rate.is_finite() && rate > 0.))
            || samples
                .iter()
                .flatten()
                .any(|value| !value.is_finite() || *value < 0.)
            || styles
                .insert(
                    id,
                    LightStyle {
                        samples,
                        samples_per_second,
                    },
                )
                .is_some()
        {
            return None;
        }
    }
    (tokens.first() == Some(&CLOSE) && !styles.is_empty()).then_some(styles)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_light_style_table_decodes_from_the_shader_cache() {
        let Ok((path, _)) = starbreaker_p4k::find_p4k() else {
            eprintln!("No local game archive; skipping live light-style fixture");
            return;
        };
        let archive = MappedP4k::open(path).unwrap();
        let shader = archive
            .read_stored_archive_file(
                r"Engine\ShaderCache_Vulkan.pak",
                "Shaders/CFXIB/Vulkan/Light.cfxb",
            )
            .expect("read Light.cfxb from the stored shader cache");
        let styles = decode(&shader).expect("decode the authored LightStyles table");
        assert!(styles.values().any(|s| s.samples.len() > 1));
        // Compare independent measurements of the LIVE Vulkan member documented in
        // WP-I1. Restrict the golden assertions to that archive revision.
        let cache = archive
            .entry_case_insensitive(r"Engine\ShaderCache_Vulkan.pak")
            .unwrap();
        if cache.crc32 == 0x54ae6542 {
            assert_eq!(styles.len(), 89);
            assert_eq!(
                styles
                    .values()
                    .map(|style| style.samples.len())
                    .sum::<usize>(),
                5299
            );
            let style = &styles[&14];
            assert_eq!(style.samples_per_second, Some(60.));
            assert_eq!(style.samples.len(), 32);
            assert_eq!(
                &style.samples[..6],
                &[[0.8; 3], [0.9; 3], [1.; 3], [0.9; 3], [0.8; 3], [0.; 3]]
            );
        }
    }

    #[test]
    fn parses_loop_rate_and_scalar_and_rgb_keys() {
        // Synthetic styles: one two-key loop and one constant RGB multiplier.
        let tokens = [
            OPEN,
            LIGHT_STYLE,
            433,
            OPEN,
            TIME_INCREMENT,
            ASSIGN,
            440,
            TERMINATOR,
            MAP,
            ASSIGN,
            OPEN,
            SEPARATOR,
            DOT,
            437,
            SEPARATOR,
            433,
            SEPARATOR,
            CLOSE,
            CLOSE,
            LIGHT_STYLE,
            434,
            OPEN,
            MAP,
            ASSIGN,
            OPEN,
            SEPARATOR,
            433,
            432,
            433,
            SEPARATOR,
            CLOSE,
            CLOSE,
            CLOSE,
        ];
        let styles = parse_table(&tokens, &HashMap::new()).unwrap();
        assert_eq!(styles[&1].samples_per_second, Some(8.));
        assert_eq!(styles[&1].samples, [[0.5; 3], [1.; 3]]);
        assert_eq!(styles[&2].samples_per_second, None);
        assert_eq!(styles[&2].samples, [[1., 0., 1.]]);
        assert!(parse_table(&tokens[..tokens.len() - 1], &HashMap::new()).is_none());
    }
}
