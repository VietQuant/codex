## Custom Prompts

Save frequently used prompts as Markdown files and reuse them quickly from the slash menu.

- Locations:
  - Project prompts: `.codex/prompts/` at your project root.
  - Personal prompts: `$CODEX_HOME/prompts/` (defaults to `~/.codex/prompts/`).
- File type: Only Markdown files with the `.md` extension are recognized.
- Name: The filename without the `.md` extension becomes the slash entry. For a file named `my-prompt.md`, type `/my-prompt`.
- Content: The file contents are sent as your message when you select the item in the slash popup and press Enter.
- Arguments: You can interpolate values typed after the command name when invoking it:
  - `$ARGUMENTS` expands to everything after the command token (trimmed of leading/trailing whitespace).
  - `$1`, `$2`, … expand to individual space-separated arguments; wrap text in quotes to keep spaces inside a single argument.
  - Placeholders with no matching argument expand to an empty string.
- How to use:
  - Start a new session (Codex loads custom prompts on session start).
  - In the composer, type `/` to open the slash popup and begin typing your prompt name.
  - Use Up/Down to select it. Press Enter to submit its contents, or Tab to autocomplete the name.
- Notes:
  - When a project prompt and a personal prompt share the same name, the project prompt takes precedence.
  - Files with names that collide with built‑in commands (e.g. `/init`) are ignored and won’t appear.
  - New or changed files are discovered on session start. If you add a new prompt while Codex is running, start a new session to pick it up.
