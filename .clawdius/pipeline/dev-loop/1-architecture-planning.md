## Step-by-Step Plan

### Step 1: Identify Untested Public Behavior
- Use `cargo test --package <crate>` to find public functions/methods without tests
- Review code coverage reports if available
- Acceptance: List of specific public functions/methods that need tests

### Step 2: Write Failing Tests
- For each identified public function/method, create a test that:
  - Calls the function with appropriate inputs
  - Asserts expected behavior
  - Initially fails (red state)
- Acceptance: Tests exist for all identified public behaviors and fail when run

### Step 3: Implement Functionality
- Make minimal changes to pass the new tests
- Avoid over-engineering or changing existing behavior
- Acceptance: New tests pass (green state)

### Step 4: Verify Test Quality
- Ensure no tests are assertion-free
- Check that tests cover edge cases and error conditions
- Acceptance: All tests have meaningful assertions and cover important scenarios

### Step 5: Targeted Testing
- Use `cargo test -p <crate> <filter>` for quick feedback
- Avoid running the entire workspace test suite
- Acceptance: Tests pass with targeted test commands