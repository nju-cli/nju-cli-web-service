{ pkgs }:

let
  codexConfigText = ''
    model = "openai/gpt-oss-120b:free"
    model_provider = "openrouter"
    approval_policy = "on-request"
    sandbox_mode = "danger-full-access"
    model_reasoning_effort = "medium"
    model_instructions_file = "custom_instructions.md"

    [model_providers.openrouter]
    name = "OpenRouter"
    base_url = "https://openrouter.ai/api/v1"
    env_key = "OPENROUTER_API_KEY"
    wire_api = "responses"

    [marketplaces.nju-cli]
    source_type = "git"
    source = "https://github.com/nju-cli/codex-marketplace.git"

    [plugins."nju-cli@nju-cli"]
    enabled = true
  '';

  customInstructionsText = ''
    You are ChatNJU, a chat agent for NanJing University students.

    - You are running inside an isolated sandbox for a single web user.
    - Use `nju-cli` for Nanjing University questions and workflows before falling back to generic web search.
    - Inspect `nju-cli --help` and subcommand help when you need exact command syntax.
    - Do not assume the user is logged in to NJU systems unless credentials or cookies are explicitly provided during the session.
    - Give a direct answer for simple queries.
    - For more complex or NJU-specific queries, you can work harder as an agent.
  '';
in
{
  inherit codexConfigText customInstructionsText;

  codexConfigFile = pkgs.writeText "codex-config.toml" codexConfigText;
  customInstructionsFile = pkgs.writeText "custom_instructions.md" customInstructionsText;
}
