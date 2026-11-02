//! Developer tool: evaluate an extension's declarativeNetRequest rulesets.
//!     cargo run -p kite-core --example dnr-check -- <ext dir> <url> [initiator] [type]
use kite_core::webext::dnr;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let dir = std::path::Path::new(&a[1]);
    let m: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("manifest.json")).unwrap()).unwrap();
    let mut set = dnr::RuleSet::default();
    for r in m["declarative_net_request"]["rule_resources"].as_array().unwrap() {
        if r["enabled"] != true { continue }
        let p = dir.join(r["path"].as_str().unwrap().trim_start_matches('/'));
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
        let (rules, errs) = dnr::parse_rules(&v);
        println!("{}: {} rules, {} errors {:?}", r["id"], rules.len(), errs.len(), errs.iter().take(2).collect::<Vec<_>>());
        set.static_rules.extend(rules);
    }
    let req = dnr::Request::new(&a[2], a.get(3).map(String::as_str).unwrap_or("http://localhost:8765/"), "GET", a.get(4).map(String::as_str).unwrap_or("script"));
    println!("outcome: {:?}", set.evaluate(&req));
}
