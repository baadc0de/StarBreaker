//! Diagnostics over `Init.bnk`'s mix: for each bus given (and its parent chain) its own props, ducks, state groups and
//! effect slots (decoded parameters and the RTPC curves on each effect's own parameters); then every Wwise Meter with
//! the bus it measures, its ballistics and the game parameter it writes, and every bus whose curves read that game
//! parameter (the side-chain targets) with the parameter they drive.
//! Usage: mix_report [bus id ...]
use std::env;

use starbreaker_wwise::hirc::HircObject;
use starbreaker_wwise::library::AudioLibrary;
use starbreaker_wwise::mix::bus_chain;
use starbreaker_wwise::playable::Resolver;

fn main() {
    let ids: Vec<u32> = env::args().skip(1).filter_map(|a| a.parse().ok()).collect();
    let p4k_path = starbreaker_common::discover::find_p4k().expect("Data.p4k").path;
    let p4k = starbreaker_p4k::MappedP4k::open(&p4k_path).expect("open p4k");
    let lib = AudioLibrary::open(&p4k).expect("library");
    let r = Resolver::new(&lib);
    let init = lib.init_bank().expect("Init.bnk");
    let banks = [init.clone()];
    let defaults = &init.game_parameter_defaults;
    let mut seen = Vec::new();
    for id in ids {
        for (bus, out) in bus_chain(&r, id) {
            if seen.contains(&bus) {
                continue;
            }
            seen.push(bus);
            let Some(HircObject::AudioBus(b) | HircObject::AuxBus(b)) = lib.object(&banks, bus) else { continue };
            println!("BUS {bus} parent={:?} props={:?} hdr_bits={:#x}", b.parent(), b.properties.props.iter().map(|p| (p.id, p.as_f32())).collect::<Vec<_>>(), b.hdr_bits);
            for d in &b.ducks {
                println!("  duck -> {} {:.1} dB out {} ms in {} ms prop {}", d.bus_id, d.duck_volume, d.fade_out_time, d.fade_in_time, d.target_prop);
            }
            for g in &b.state_chunk.state_groups {
                println!("  state group {} {:?}", g.group_id, g.states.iter().map(|s| (s.state_id, s.properties.props.clone())).collect::<Vec<_>>());
            }
            for c in &out.rtpc {
                println!("  rtpc {} ({:#x}) -> {} scaling {} accum {} {:?} default {:?}", c.rtpc, c.rtpc_id, c.param, c.scaling, c.accum, c.points, defaults.get(&c.rtpc_id));
            }
            for e in &out.effects {
                println!("  fx slot {} {} {} bypass={} state_bypass={:?} eq={:?} meter={:?} limiter={:?} gain={:?}", e.slot, e.id, e.kind, e.bypass, e.state_bypass, e.eq, e.meter, e.limiter, e.gain_db);
                for c in &e.rtpc {
                    println!("    fx rtpc {} ({:#x}) -> {} scaling {} accum {} {:?} default {:?}", c.rtpc, c.rtpc_id, c.param, c.scaling, c.accum, c.points, defaults.get(&c.rtpc_id));
                }
                if let Some(HircObject::FxShareSet(f) | HircObject::FxCustom(f)) = lib.object(&banks, e.id) {
                    for g in &f.state_chunk.state_groups {
                        println!("    fx state group {} {:?}", g.group_id, g.states.iter().map(|s| (s.state_id, s.properties.props.clone())).collect::<Vec<_>>());
                    }
                    if !f.state_property_values.is_empty() {
                        println!("    fx state props {:?}", f.state_property_values);
                    }
                }
            }
        }
    }
    // Meters: who measures what, and who listens.
    let all: Vec<u32> = init.hierarchy.objects().filter_map(|o| matches!(o, HircObject::AudioBus(_) | HircObject::AuxBus(_)).then(|| o.id())).collect();
    for &bus in &all {
        let chain = bus_chain(&r, bus);
        let Some((_, out)) = chain.first() else { continue };
        for e in &out.effects {
            let Some(m) = &e.meter else { continue };
            println!("METER bus={bus} slot={} fx={} bypass={} -> {} ({:#x}) default {:?} attack={} release={} hold={} min={} max={} mode={} scope={} downstream={}",
                e.slot, e.id, e.bypass, m.game_parameter, m.game_parameter_id, defaults.get(&m.game_parameter_id), m.attack_s, m.release_s, m.hold_s, m.min_db, m.max_db, m.mode, m.scope, m.apply_downstream_volume);
            for &t in &all {
                let tc = bus_chain(&r, t);
                let Some((_, to)) = tc.first() else { continue };
                for c in to.rtpc.iter().filter(|c| c.rtpc_id == m.game_parameter_id) {
                    println!("  target bus={t} {} scaling {} {:?}", c.param, c.scaling, c.points);
                }
                for fe in &to.effects {
                    for c in fe.rtpc.iter().filter(|c| c.rtpc_id == m.game_parameter_id) {
                        println!("  target bus={t} fx {} {} {} {:?}", fe.id, fe.kind, c.param, c.points);
                    }
                }
            }
        }
    }
}
