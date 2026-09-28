use std::env;

// Canary program-id selection, klend-exact: plain-staging Scope mirrors against the PROD Canary
// (prerelease check), so only the dedicated `staging-to-staging` cluster selects the staging
// Canary. `--features staging` stays an explicit opt-in.
fn main() {
    if cfg!(feature = "staging") {
        return;
    }
    println!("cargo:rerun-if-env-changed=CLUSTER");
    if env::var("CLUSTER").as_deref() == Ok("staging-to-staging") {
        println!("cargo:rustc-cfg=feature=\"staging\"");
    }
}
