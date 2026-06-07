{ pkgs }:

let
  codexConfigText = ''
    model = "openai/gpt-oss-120b:free"
    model_provider = "openrouter"
    approval_policy = "on-request"
    sandbox_mode = "danger-full-access"
    model_reasoning_effort = "medium"
    model_instructions_file = "custom_instructions.md"
    model_catalog_json = "model-catalog.json"

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

  modelCatalogText = ''
    {
      "models": [
        {
          "slug": "openai/gpt-oss-120b:free",
          "display_name": "openai/gpt-oss-120b:free",
          "description": "OpenRouter gpt-oss free model; text input only.",
          "default_reasoning_level": "medium",
          "supported_reasoning_levels": [
            { "effort": "low", "description": "Fast responses with lighter reasoning" },
            { "effort": "medium", "description": "Balances speed and reasoning depth" },
            { "effort": "high", "description": "Greater reasoning depth for complex tasks" }
          ],
          "shell_type": "shell_command",
          "visibility": "list",
          "supported_in_api": true,
          "priority": 100,
          "availability_nux": null,
          "upgrade": null,
          "base_instructions": "",
          "supports_reasoning_summaries": false,
          "support_verbosity": false,
          "default_verbosity": null,
          "apply_patch_tool_type": "freeform",
          "web_search_tool_type": "text",
          "truncation_policy": { "mode": "tokens", "limit": 10000 },
          "supports_parallel_tool_calls": true,
          "context_window": 128000,
          "max_context_window": 128000,
          "experimental_supported_tools": [],
          "input_modalities": ["text"],
          "supports_search_tool": false
        }
      ]
    }
  '';

  customInstructionsText = ''
    You are ChatNJU, a chat agent for NanJing University students.

    - Give a direct answer for simple queries.
    - For more complex or NJU-specific queries, you can work harder as an agent.
    - For image understanding tasks, spawn the `image_understanding` subagent.
    - For PDF files, use cli tools to convert to text (pdftotext) or image (pdftoppm) to view them. poppler-utils are already installed in your environment.
  '';

  imageUnderstandingAgentText = ''
    name = "image_understanding"
    description = "Image understanding specialist for describing, reading, and reasoning about images, screenshots, scans, diagrams, and visual attachments."
    model = "google/gemma-4-31b-it:free"
    model_reasoning_effort = "medium"
    developer_instructions = """
    You are an image understanding specialist.
    Focus on visual analysis: describe image content, extract text from screenshots or scans, interpret charts and diagrams, and answer questions grounded in the image.
    Be explicit about uncertainty when details are blurry, occluded, or too small to read.
    Do not make code changes unless the parent agent specifically asks you to.
    """
  '';
in
{
  inherit codexConfigText modelCatalogText customInstructionsText imageUnderstandingAgentText;

  codexConfigFile = pkgs.writeText "codex-config.toml" codexConfigText;
  modelCatalogFile = pkgs.writeText "model-catalog.json" modelCatalogText;
  customInstructionsFile = pkgs.writeText "custom_instructions.md" customInstructionsText;
  imageUnderstandingAgentFile = pkgs.writeText "image-understanding-agent.toml" imageUnderstandingAgentText;
}
