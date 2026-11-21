# Dependency Upgrade Loop Plan

## Step-by-Step Plan

1. **Identify Updates**
   - Run `cargo update --dry-run`
   - Document all dependencies with newer patch/minor versions

2. **Process Each Dependency Individually**
   - Update one dependency in its Cargo.toml
   - Run `cargo check -p <crate>` for quick feedback
   - If fails: revert change, record reason, skip to next dependency
   - If passes: proceed to next dependency

3. **Final Verification**
   - After updating all possible dependencies:
     - Run `cargo check` for workspace compilation
     - Let pipeline run full test suite and gates
     - If pipeline passes: pipeline commits, pushes, opens PR
     - If pipeline fails: revert all changes, record reasons

## Acceptance Criteria

- [ ] Only patch/minor version upgrades performed
- [ ] No major version upgrades attempted
- [ ] Each dependency tested individually before proceeding
- [ ] All `cargo check -p <crate>` passes for updated crates
- [ ] Full workspace compiles (`cargo check`)
- [ ] No manual git operations performed
- [ ] Failed upgrades reverted with reasons recorded
- [ ] Pipeline handles commit/push/PR creation