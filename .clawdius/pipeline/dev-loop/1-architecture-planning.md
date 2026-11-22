## Dependency Upgrade Loop Plan

### Steps:
1. **Identify Updates**
   - Run `cargo update --dry-run`
   - Capture list of dependencies with available patch/minor updates

2. **Process Single Dependency**
   - Select next dependency from list
   - Update version in Cargo.toml
   - Run `cargo check -p <crate>`

3. **Evaluate Result**
   - If check passes: mark as complete, proceed to next
   - If check fails: revert change, record failure reason, proceed to next

4. **Repeat Until Complete**
   - Continue with remaining dependencies

### Acceptance Criteria:
- All dependencies with available patch/minor updates processed
- No major version upgrades performed
- Any breaking changes reverted with reason recorded
- Final workspace compiles with `cargo check`
- No manual git operations performed
- Changes left in working tree for pipeline to handle