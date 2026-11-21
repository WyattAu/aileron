# Plan: Add Focused Tests for Untested Public Behavior

## Step 1: Identify Untested Public Behavior
- Run `cargo test --workspace` to find public functions without test coverage
- Review documentation and code comments for expected behavior
- List specific public APIs lacking tests

**Acceptance Criteria**: Clear list of at least 3 public functions/methods without tests

## Step 2: Write Failing Tests
- Create test cases targeting identified public APIs
- Ensure tests fail with clear error messages
- Include edge cases and boundary conditions

**Acceptance Criteria**: At least 3 new tests that fail before implementation

## Step 3: Implement Functionality
- Make minimal changes to pass the tests
- Focus only on what the tests require
- Avoid over-engineering

**Acceptance Criteria**: All new tests pass with the implementation

## Step 4: Verify Test Quality
- Run `cargo test -p <crate> <filter>` to confirm tests pass
- Check that tests provide meaningful coverage
- Ensure no assertion-free tests exist

**Acceptance Criteria**: All tests pass and provide meaningful coverage of public behavior