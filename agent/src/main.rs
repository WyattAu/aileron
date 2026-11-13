//! aileron-agent: headless MCP daemon over aileron-core + aileron-engine.

fn main() -> anyhow::Result<()> {
    aileron_agent::run()
}
