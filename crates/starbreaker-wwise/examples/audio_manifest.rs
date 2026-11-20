//! Build an audio manifest for triggers and environments from the game archive and summarise it.
//! Usage: audio_manifest <triggers.txt (one name per line, first tab field)> <out dir> [ENV_NAME...]
//! Writes `<out>/audio.json` and the media / impulse-response payloads under `<out>/`.
use std::{env, fs, path::Path, time::Instant};

use starbreaker_wwise::library::AudioLibrary;
use starbreaker_wwise::manifest::{EnvironmentRecord, ManifestBuilder};

fn main() {
    let args: Vec<String> = env::args().collect();
    let (list, out) = (&args[1], Path::new(&args[2]));
    let p4k_path = starbreaker_common::discover::find_p4k().expect("Data.p4k").path;
    let p4k = starbreaker_p4k::MappedP4k::open(&p4k_path).expect("open p4k");
    let t0 = Instant::now();
    let lib = AudioLibrary::open(&p4k).expect("library");
    eprintln!("library: {} triggers in {:.1}s", lib.atl.len(), t0.elapsed().as_secs_f32());
    let mut builder = ManifestBuilder::new(&lib, "Audio");
    let mut counts = std::collections::BTreeMap::<&str, usize>::new();
    for line in fs::read_to_string(list).unwrap().lines() {
        let name = line.split('\t').next().unwrap().trim();
        if name.is_empty() {
            continue;
        }
        let status = builder.trigger(name);
        *counts.entry(status).or_default() += 1;
        if status != "playable" {
            println!("{status}\t{name}");
        }
    }
    for env_name in &args[3..] {
        builder.environment(env_name, EnvironmentRecord { record: env_name.clone(), ..Default::default() });
    }
    let t1 = Instant::now();
    let (manifest, payloads) = builder.finish();
    eprintln!("finish: {:.1}s, {} payloads, {:.1} MB", t1.elapsed().as_secs_f32(), payloads.len(), payloads.iter().map(|p| p.bytes.len()).sum::<usize>() as f32 / 1e6);
    println!("{counts:?}");
    println!("nodes {} attenuations {} buses {} media {} rtpcs {}", manifest.nodes.len(), manifest.attenuations.len(), manifest.buses.len(), manifest.media.len(), manifest.rtpcs.len());
    let errors = manifest.media.values().filter(|m| m.error.is_some()).count();
    println!("media errors {errors}");
    for (name, e) in &manifest.environments {
        println!("env {name}: {}", serde_json::to_string(e).unwrap());
    }
    fs::create_dir_all(out).unwrap();
    fs::write(out.join("audio.json"), serde_json::to_vec_pretty(&manifest).unwrap()).unwrap();
    for p in payloads {
        let path = out.join(&p.path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, p.bytes).unwrap();
    }
}
