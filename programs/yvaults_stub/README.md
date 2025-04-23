## yvaults_stub

This is a stub dependency for yvaults.

It is only required when compiling scope without having access to the yvaults repo.

To use this dependency, simply change the scope Cargo.toml to use this package instead of the yvaults repo.

```toml
[dependencies]
# Comment out the git repo
#yvaults = { git = "ssh://git@github.com/Kamino-Finance/yvaults.git", features = ["no-entrypoint", "cpi"], optional = true }

# Add this line
yvaults = { path = "../yvaults_stub", package = "yvaults_stub", optional = true }
```
