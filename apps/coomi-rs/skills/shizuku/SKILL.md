---
name: shizuku
description: Run Coomi's Android Shizuku self-check script and explain its result to the AI. Use when the user asks to check Shizuku, asks for an operation that may use Shizuku, or the AI is unsure how to determine the current Shizuku server, rish environment, or authorization state.
---

# Shizuku self-check

This Skill packages `scripts/shizuku_check.sh`. It gives the AI a repeatable way to inspect the current Coomi Shizuku environment and the information needed to decide how to continue a user-requested task. It does not implement a new privileged operation and it does not choose an operation on the user's behalf.

## Use the Skill

1. Load this Skill when the user asks to check Shizuku, asks how to use Shizuku, or mentions a task that may depend on Shizuku while the current state is unknown.
2. Run the bundled script in Coomi's Android shell:

   ```sh
   sh "$HOME/.coomi/skills/shizuku/scripts/shizuku_check.sh"
   ```

   The script is Android-specific. `$HOME` is Coomi's Termux-compatible home on the phone; do not substitute a Windows or WSL path when the task is about the phone.
3. Read the `SHIZUKU_STATE=...` line from stdout, the exit code, and any diagnostic text on stderr. Use those facts to explain the current state to the user or to select the next step for the user's requested operation.
4. The script accepts the options documented in its own header, including `-v` for diagnostics and `--fix` for its optional dex deployment repair. Do not invent other options or output formats.

## Result contract

| Exit code | State | Meaning |
| ---: | --- | --- |
| 0 | `AVAILABLE` | The end-to-end Shizuku probe completed for the current Coomi application. |
| 1 | `SERVER_NOT_RUNNING` | The probe could not connect to a running Shizuku server. |
| 2 | `NOT_GRANTED` | Shizuku is reachable but the current application is not authorized. |
| 3 | `ENV_MISSING` | The rish dex or Android process environment required by the probe is missing or unusable. |
| 4 | `UNKNOWN` | The probe timed out or returned an error that it could not classify. |

The normal stdout contract is one `SHIZUKU_STATE=...` line. Detailed diagnostics are written to stderr. Preserve the original state, exit code, and diagnostic text when reporting the result.

## Scope

Use this Skill as a reference and self-check helper. After the check, follow the user's explicit request and the available Coomi/Android tooling for any further action; the Skill itself only checks and explains the Shizuku environment.

## Resource

The source script is `scripts/shizuku_check.sh`. Invoke it with `sh` so it works after Skill extraction even when the executable bit is not preserved.
