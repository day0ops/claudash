# Setup claudash

Install and configure the claudash plugin as your Claude Code statusline.

Re-running this command will update claudash to the latest version.

## Steps

1. **Check if claudash is already installed.**

Check if a `claudash` binary already exists on the system:

```bash
which claudash
```

If found, note the path — the update should replace the binary at that location.

If not found, default to `~/.local/bin/claudash`.

2. **Download the latest pre-built binary from GitHub releases.**

Detect OS and architecture:

```bash
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)
case "$ARCH" in
  x86_64) ARCH="amd64" ;;
  aarch64) ARCH="arm64" ;;
esac
```

Download and install to the target directory (the directory from step 1):

```bash
mkdir -p <target_directory>
curl -fsSL "https://github.com/day0ops/claudash/releases/latest/download/claudash-${OS}-${ARCH}" -o <target_directory>/claudash
chmod +x <target_directory>/claudash
```

On Windows, the binary name is `claudash-windows-${ARCH}.exe`.

If the download fails (e.g. `curl` is not available, no internet, or the
platform is unsupported), fall back to building from source:

```bash
cargo install --git https://github.com/day0ops/claudash.git
```

This requires Rust to be installed.

3. **Verify the binary is available.**

```bash
<target_directory>/claudash --version
```

4. **Ask the user about their terminal background.**

Ask the user whether they use a **dark** or **light** terminal background. If
they use a light background, include the `--light` flag in the command (this
switches to a darker color palette with better contrast on white/light
backgrounds).

5. **Configure `settings.json` if not already set.**

Determine the settings file path: use `$CLAUDE_CONFIG_DIR/settings.json` if the
`CLAUDE_CONFIG_DIR` environment variable is set, otherwise
`~/.claude/settings.json`.

Read the settings file. If `statusLine` is already configured and its `command`
contains `claudash`, **skip this step** — the existing configuration is valid.

Otherwise, set the `statusLine` field. If the target directory is in the user's
`$PATH`, use just the binary name:

For dark terminal backgrounds:

```json
{
  "statusLine": {
    "type": "command",
    "command": "claudash"
  }
}
```

For light terminal backgrounds:

```json
{
  "statusLine": {
    "type": "command",
    "command": "claudash --light"
  }
}
```

If the target directory is not in `$PATH`, use the full path instead of `claudash`.

Preserve all other fields in the file.

6. **Confirm** the change was made and tell the user to restart their Claude
   Code session for the statusline to take effect. Mention that they can add
   extra flags like `--cwd` and `--git-branch` to their command later.
