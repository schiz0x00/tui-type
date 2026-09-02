# Security policy

Please report security-sensitive issues privately through GitHub's security-advisory interface rather than a public issue.

The project keeps `Cargo.lock` under version control. CI rejects RustSec vulnerabilities, warnings, and yanked dependencies with `cargo audit --deny warnings`. Dependency updates are proposed weekly by Dependabot; advisory suppressions are not accepted without a documented reachability analysis, expiry condition, and upstream tracking link.

Custom text and word lists are limited to 1 MiB, configuration to 64 KiB, and history to 4 MiB and 1,000 records. History writes use a cross-process lock and an atomic replacement. Malformed history is preserved under a unique `history.json.corrupt-*` name before a new file is created.
