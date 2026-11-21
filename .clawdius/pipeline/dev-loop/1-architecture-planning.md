## Documentation Sync Plan

### Steps:
1. **Inventory existing documentation**
   - List all files under docs/ directory
   - Identify README.md location and structure

2. **Scan codebase for commands and flags**
   - Identify CLI entry points and main commands
   - Extract current flags and options from code
   - Note any version-specific commands

3. **Cross-reference documentation with code**
   - Verify commands in docs match actual implementation
   - Check if all documented flags still exist
   - Identify broken cross-references (links to non-existent sections/files)

4. **Update documentation in place**
   - Fix stale commands with current syntax
   - Add missing flags or remove deprecated ones
   - Repair broken cross-references
   - Ensure examples match current behavior

5. **Validate changes**
   - Check that all updated commands are syntactically correct
   - Verify examples produce expected output
   - Confirm all internal links resolve properly

### Acceptance Criteria:
- [ ] All commands in docs/README.md match current implementation
- [ ] All documented flags exist in the codebase
- [ ] No broken cross-references within documentation
- [ ] Examples produce expected results
- [ ] No new files created under docs/ or repo root
- [ ] Changes are minimal and focused on accuracy only