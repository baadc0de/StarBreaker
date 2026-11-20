//! Diagnostics: the playback limits along triggers' play trees — for every node from each played node up to its top
//! ancestor: `AdvSettingsParams` (max instances, kill-newest, virtual behaviour, the global-limit bit, ignore-parent) —
//! to see how many voices of one sound the game allows at once.
//! Usage: limits_report <trigger> ...
use std::env;

use starbreaker_wwise::library::AudioLibrary;
use starbreaker_wwise::playable::{Resolver, node_base};

fn main() {
    let p4k_path = starbreaker_common::discover::find_p4k().expect("Data.p4k").path;
    let p4k = starbreaker_p4k::MappedP4k::open(&p4k_path).expect("open p4k");
    let lib = AudioLibrary::open(&p4k).expect("library");
    let mut r = Resolver::new(&lib);
    for name in env::args().skip(1) {
        let t = r.trigger(&name).clone();
        println!("TRIGGER {name} bank={} status={}", t.bank, t.status);
        let banks: Vec<_> = lib.bank(&t.bank).into_iter().chain(lib.init_bank()).collect();
        let mut stack: Vec<usize> = t.play.iter().map(|p| p.root).collect();
        let mut seen = Vec::new();
        while let Some(i) = stack.pop() {
            let node = r.out.nodes[i].clone();
            stack.extend(node.children.iter().copied());
            let mut id = node.id;
            let mut depth = 0;
            while id != 0 && depth < 12 {
                if seen.contains(&id) {
                    break;
                }
                seen.push(id);
                let Some(obj) = lib.object(&banks, id) else { break };
                let Some(b) = node_base(obj) else { break };
                let a = &b.adv_settings;
                println!(
                    "  {}node {id} {} max_instances={} kill_newest={} use_virtual={} bit2_global={} ignore_parent={} virtual_queue={} below_threshold={}",
                    "  ".repeat(depth),
                    obj.type_name(),
                    a.max_num_instance,
                    a.flags1 & 1 != 0,
                    a.flags1 & 2 != 0,
                    a.flags1 & 4 != 0,
                    a.flags1 & 8 != 0,
                    a.virtual_queue_behavior,
                    a.below_threshold_behavior
                );
                id = b.direct_parent_id;
                depth += 1;
            }
        }
    }
}
