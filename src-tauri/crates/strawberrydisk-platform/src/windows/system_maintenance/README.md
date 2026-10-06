# Windows maintenance steps

Privileged tasks declare their operations in `task_scripts.rs`. `steps.ps1` owns step execution, service snapshots, bounded diagnostics, bounded failure output, and progress reporting. Recipes are compiled into the application; the helper accepts task IDs, never script text or executable paths.

Use `Invoke-StrawberryService` for service operations, `Invoke-StrawberryNative` for system executables, and `Invoke-StrawberryStep` for other fixed platform operations. Mark post-execution checks as `Verify` and run them under the same privileges as the operation. Native executables must resolve from the Windows directory supplied by the platform API. Do not silently enable disabled services.

A recipe stops on failure. A `finally` block may restore a service explicitly stopped by that recipe; each recovery action must also use the shared step functions. Evidence from the original failure remains available if recovery fails too.

If the diagnostic channel fails, retain the steps already accepted and stop reading that channel. Telemetry failure must not turn a successful native operation into a maintenance failure.

Keep diagnostic components and actions in the typed allowlist in `step_diagnostics.rs`. Retain the last 512 characters of failed native output or the exception message, including useful paths; clear this text for successful steps. Never pass credentials, file contents, or arbitrary command arguments. The v4 helper protocol rejects older peers before executing tasks and reserves a 128 KiB response budget for up to 32 steps. Tool exit codes are not automatically Win32 errors: add a user-facing reason only when the error source establishes that meaning.

Validate new recipes with the PowerShell parser, shared-runner fixtures, failure-classification tests, and an applicable Windows environment. Real maintenance tests are ignored by default and require an explicitly authorized test host. Explorer cache refresh remains on the cancellable local-command path; it shares command-failure and verification classification, but does not use the privileged step runner.
