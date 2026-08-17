use std::env;

// klend program-id selection by target cluster.
//
// Scope staging deliberately targets klend *prod* — the scope-staging deployment mirrors against the
// live klend program as a prerelease check — so `CLUSTER=staging` leaves klend at its mainnet id.
// Only the dedicated `staging-to-staging` cluster (scope staging <-> klend staging) selects the
// klend staging id. The `staging` Cargo feature also remains an explicit opt-in.
fn main() {
    if cfg!(feature = "staging") {
        // The staging feature has been set explicitly, just ignore env variables.
        return;
    }

    // Rerun if CLUSTER is changed
    println!("cargo:rerun-if-env-changed=CLUSTER");
    // Only staging-to-staging selects klend staging; plain staging stays on klend prod.
    if env::var("CLUSTER").as_deref() == Ok("staging-to-staging") {
        println!("cargo:rustc-cfg=feature=\"staging\"");
    }
}
