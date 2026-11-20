use ceh_browser::evidence::{summarize_cookies, summarize_environment};
use serde_json::json;

#[test]
fn cookies_are_reported_by_name_domain_and_expiry_never_by_value() {
    let result = json!({"cookies":[
        {"name":"JSESSIONID","value":{"type":"string","value":"SECRET-SESSION"},"domain":"icp.administracionelectronica.gob.es","path":"/","size":40,"httpOnly":true,"secure":true,"sameSite":"none","expiry":1794400000},
        {"name":"TS01abc","value":{"type":"string","value":"SECRET-TS"},"domain":".administracionelectronica.gob.es","path":"/","httpOnly":false,"secure":true,"sameSite":"lax"},
        {"name":"lookalike","value":{"type":"string","value":"SECRET-LOOK"},"domain":"xadministracionelectronica.gob.es","path":"/"},
        {"name":"lookalike2","value":{"type":"string","value":"SECRET-LOOK2"},"domain":"administracionelectronica.gob.es.evil.test","path":"/"},
        {"name":"other","value":{"type":"string","value":"SECRET-OTHER"},"domain":"example.com","path":"/","httpOnly":false,"secure":false,"sameSite":"lax"}]});
    let summary = summarize_cookies(&result);
    assert_eq!(summary.as_array().unwrap().len(), 2, "only the site's domains");
    assert_eq!(summary[0]["name"], "JSESSIONID");
    assert_eq!(summary[0]["expiry"], 1794400000);
    assert_eq!(summary[0]["http_only"], true);
    assert!(!summary.to_string().contains("SECRET"));
}

#[test]
fn the_environment_keeps_only_the_known_fields() {
    let raw = r#"{"visibility":"hidden","has_focus":false,"inner":[1120,700],"outer":[1120,850],"dpr":2,"webdriver":true,"languages":["es-ES","es"],"timezone":"Europe/Madrid","user_agent":"Mozilla/5.0","platform":"MacIntel","cookie":"SECRET","form":{"nie":"SECRET"}}"#;
    let environment = summarize_environment(raw).unwrap();
    assert_eq!(environment["visibility"], "hidden");
    assert_eq!(environment["has_focus"], false);
    assert_eq!(environment["webdriver"], true);
    assert!(!environment.to_string().contains("SECRET"));
    assert!(summarize_environment("not json").is_none());
}

#[test]
fn evidence_scripts_only_read_the_page() {
    let source = include_str!("../src/evidence.rs");
    for forbidden in [".click(", ".submit(", "dispatchEvent", ".value =", ".value=", "location.assign", "location.href =", "localStorage.set", "document.cookie ="] {
        assert!(!source.contains(forbidden), "evidence.rs must not contain {forbidden}");
    }
}

#[test]
fn the_protection_cookie_of_the_finished_check_is_the_one_that_counts() {
    use ceh_browser::evidence::challenge_cookies_ready;
    use serde_json::json;
    let names = |list: &[&str]| json!(list.iter().map(|n| json!({"name": n, "domain": "x"})).collect::<Vec<_>>());
    assert!(challenge_cookies_ready(&names(&["TSPD_101", "TSPD_101_DID", "JSESSIONID"])));
    assert!(challenge_cookies_ready(&names(&["TSPD_101"])), "f5_cspm comes much later on this site, it is not waited for");
    assert!(!challenge_cookies_ready(&names(&["TSPD_101_DID", "TS01629f81"])), "the device cookie alone does not say the check is done");
    assert!(!challenge_cookies_ready(&names(&["f5_cspm"])));
    assert!(!challenge_cookies_ready(&json!(null)));
    assert!(!challenge_cookies_ready(&names(&[])));
}
