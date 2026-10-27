use crate::report::operation_log::ActionStatus;
use anyhow::{Context, Result};
use std::process::Command;

/// A single, fully-typed system command this action would run.
///
/// Deliberately not a shell string: no glob expansion, no `sh -c` interpolation,
/// no chance that a crafted path turns into an extra command. `program` is a
/// fixed binary name baked into the action at compile time; only `args` is ever
/// built at runtime, and always as a plain `Vec<String>` passed straight to argv.
#[derive(Debug, Clone)]
pub struct SystemCommand {
    pub program: &'static str,
    pub args: Vec<String>,
}

#[allow(dead_code)]
impl SystemCommand {
    pub fn new(program: &'static str, args: Vec<String>) -> Self {
        Self { program, args }
    }

    /// Human-readable rendering for the dry-run preview and the audit log.
    /// Display only — this string is never parsed back or fed to a shell.
    pub fn display(&self) -> String {
        if self.args.is_empty() {
            self.program.to_string()
        } else {
            format!("{} {}", self.program, self.args.join(" "))
        }
    }

    /// Run the command and return its captured, trimmed stdout.
    ///
    /// Several maintenance actions are read-only diagnostics (SMART status,
    /// volume verification) where the output *is* the result — not just
    /// whether the process exited zero — so the executor needs it, not just
    /// a pass/fail bit.
    fn run(&self) -> Result<String> {
        let output = Command::new(self.program)
            .args(&self.args)
            .output()
            .with_context(|| format!("Failed to spawn `{}`", self.display()))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!(
                "`{}` exited with {}: {}",
                self.display(),
                output.status,
                stderr.trim()
            );
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }
}

/// How disruptive an action is if something goes wrong, or simply by doing what it says.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RiskLevel {
    /// Side-effect free, or trivially self-healing (e.g. rebuilds something the OS
    /// regenerates on its own).
    Low,
    /// Visible, possibly slow or disruptive, but not destructive.
    Medium,
    /// Destroys state the user cannot get back (e.g. deletes restore points).
    High,
}

impl RiskLevel {
    /// Everything above `Low` interrupts the user for an explicit yes when
    /// `--execute` is passed — `--yes` is the only way around it.
    pub fn requires_confirmation(&self) -> bool {
        !matches!(self, RiskLevel::Low)
    }

    pub fn label(&self) -> &'static str {
        match self {
            RiskLevel::Low => "low",
            RiskLevel::Medium => "medium",
            RiskLevel::High => "high",
        }
    }
}

/// Groups of maintenance actions. Deliberately separate from `CleanCategory`:
/// these are not "a path with a byte size", they are system commands.
///
/// Maps onto the catalog in `docs/MAINTENANCE_ROADMAP.md` — keep the two in sync
/// when a new category of action is ported.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ActionCategory {
    /// Catalog A — rebuild/reindex system databases (LaunchServices, Mail, locate, …).
    SystemIndex,
    /// Catalog B — caches reached via a command rather than a plain path (e.g. font caches).
    ExtendedCache,
    /// Catalog C — disk / Time Machine housekeeping.
    DiskMaintenance,
    /// Catalog D — system state (DNS cache, memory, process accounting).
    SystemState,
}

#[allow(dead_code)]
impl ActionCategory {
    pub fn name(&self) -> &'static str {
        match self {
            ActionCategory::SystemIndex => "System Index Rebuild",
            ActionCategory::ExtendedCache => "Extended System Caches",
            ActionCategory::DiskMaintenance => "Disk & Time Machine Maintenance",
            ActionCategory::SystemState => "System State",
        }
    }

    pub fn identifier(&self) -> &'static str {
        match self {
            ActionCategory::SystemIndex => "system-index",
            ActionCategory::ExtendedCache => "extended-cache",
            ActionCategory::DiskMaintenance => "disk-maintenance",
            ActionCategory::SystemState => "system-state",
        }
    }
}

/// Outcome of running one `MaintenanceAction`, shaped for the same audit log the
/// `Cleaner` pipeline already writes to (`report/operation_log.rs`).
#[derive(Debug, Clone)]
pub struct MaintenanceOutcome {
    pub action_name: String,
    pub status: ActionStatus,
    /// Rendered commands actually run, in order — for the audit trail.
    pub commands_run: Vec<String>,
    /// Captured stdout from the commands that produced any — the whole point
    /// of a read-only diagnostic action (e.g. SMART status). `None` for
    /// actions whose commands are silent on success.
    pub output: Option<String>,
}

/// Core trait implemented by every maintenance action.
///
/// Unlike `Cleaner`, there is no "scan, then act on the reviewed list": the
/// command plan for a given action is static, so `commands()` is both what the
/// dry-run preview shows and what `execute()` actually runs — nothing is
/// derived separately at execution time, so preview and reality can never drift.
#[allow(dead_code)]
pub trait MaintenanceAction: Send + Sync {
    /// Friendly display name.
    fn name(&self) -> &'static str;

    /// Category for filtering — see `docs/MAINTENANCE_ROADMAP.md`.
    fn category(&self) -> ActionCategory;

    /// Detailed description of what this action does and why.
    fn description(&self) -> &'static str;

    /// How disruptive this action is.
    fn risk(&self) -> RiskLevel;

    /// Whether this action needs elevated (admin/root) privileges to succeed.
    /// Actions that require it are skipped unless the caller explicitly passed
    /// `--allow-admin` — this trait never escalates privileges on its own, and
    /// never prompts for or handles a password itself (the OS `sudo`/Authorization
    /// prompt does that, if and when the command itself triggers one).
    fn requires_admin(&self) -> bool {
        false
    }

    /// Whether the effect can be undone (even if only by the OS regenerating
    /// what was removed) or is permanent (e.g. a deleted restore point).
    fn is_reversible(&self) -> bool;

    /// Whether the binary/service this action needs is actually present on
    /// this machine.
    fn is_available(&self) -> bool {
        true
    }

    /// The exact, ordered list of commands this action runs.
    fn commands(&self) -> Vec<SystemCommand>;

    /// Whether it is currently safe to run this action — e.g. a conflicting
    /// app has a file open that a command below would touch. Checked right
    /// before `execute()` runs anything; failing this is not an error, the
    /// caller gets a `Skipped` outcome back instead.
    fn preflight(&self) -> Result<(), String> {
        Ok(())
    }

    /// Run `preflight()`, then `commands()` in order, stopping at the first failure.
    fn execute(&self) -> Result<MaintenanceOutcome> {
        if let Err(reason) = self.preflight() {
            return Ok(MaintenanceOutcome {
                action_name: self.name().to_string(),
                status: ActionStatus::Skipped(reason),
                commands_run: Vec::new(),
                output: None,
            });
        }
        run_system_commands(self.name(), self.commands())
    }
}

/// Shared execution loop: run each command in order, stop at the first failure,
/// and report exactly which commands actually ran.
///
/// Lives outside the trait so an action that needs an extra step around the
/// core loop (e.g. a pre-flight check) can still reuse this instead of
/// duplicating the run/stop/report logic.
pub fn run_system_commands(
    action_name: &str,
    commands: Vec<SystemCommand>,
) -> Result<MaintenanceOutcome> {
    let mut commands_run = Vec::new();
    let mut outputs = Vec::new();

    for command in &commands {
        commands_run.push(command.display());
        match command.run() {
            Ok(stdout) => {
                if !stdout.is_empty() {
                    outputs.push(stdout);
                }
            }
            Err(err) => {
                return Ok(MaintenanceOutcome {
                    action_name: action_name.to_string(),
                    status: ActionStatus::Failed(err.to_string()),
                    commands_run,
                    output: (!outputs.is_empty()).then(|| outputs.join("\n")),
                });
            }
        }
    }

    Ok(MaintenanceOutcome {
        action_name: action_name.to_string(),
        status: ActionStatus::Success,
        commands_run,
        output: (!outputs.is_empty()).then(|| outputs.join("\n")),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fake action used only to exercise the trait/registry plumbing before any
    /// real action exists (those land starting Fase M1 — see
    /// `docs/MAINTENANCE_ROADMAP.md`).
    struct FakeAction {
        risk: RiskLevel,
        admin: bool,
    }

    impl MaintenanceAction for FakeAction {
        fn name(&self) -> &'static str {
            "fake-action"
        }
        fn category(&self) -> ActionCategory {
            ActionCategory::SystemState
        }
        fn description(&self) -> &'static str {
            "test-only action"
        }
        fn risk(&self) -> RiskLevel {
            self.risk
        }
        fn requires_admin(&self) -> bool {
            self.admin
        }
        fn is_reversible(&self) -> bool {
            true
        }
        fn commands(&self) -> Vec<SystemCommand> {
            vec![SystemCommand::new("/usr/bin/true", vec![])]
        }
    }

    struct FailingAction;

    impl MaintenanceAction for FailingAction {
        fn name(&self) -> &'static str {
            "failing-action"
        }
        fn category(&self) -> ActionCategory {
            ActionCategory::SystemState
        }
        fn description(&self) -> &'static str {
            "always fails"
        }
        fn risk(&self) -> RiskLevel {
            RiskLevel::Low
        }
        fn is_reversible(&self) -> bool {
            true
        }
        fn commands(&self) -> Vec<SystemCommand> {
            vec![SystemCommand::new("/usr/bin/false", vec![])]
        }
    }

    #[test]
    fn test_execute_runs_exactly_the_previewed_commands() {
        let action = FakeAction {
            risk: RiskLevel::Low,
            admin: false,
        };
        let preview: Vec<String> = action.commands().iter().map(|c| c.display()).collect();

        let outcome = action.execute().unwrap();

        assert!(matches!(outcome.status, ActionStatus::Success));
        assert_eq!(outcome.commands_run, preview);
    }

    #[test]
    fn test_failing_command_is_reported_not_panicked() {
        let outcome = FailingAction.execute().unwrap();
        assert!(matches!(outcome.status, ActionStatus::Failed(_)));
    }

    #[test]
    fn test_risk_level_confirmation_policy() {
        assert!(!RiskLevel::Low.requires_confirmation());
        assert!(RiskLevel::Medium.requires_confirmation());
        assert!(RiskLevel::High.requires_confirmation());
    }

    #[test]
    fn test_system_command_display_never_quotes_or_shells_out() {
        let cmd = SystemCommand::new("/usr/bin/true", vec!["-x".to_string(), "a b".to_string()]);
        assert_eq!(cmd.display(), "/usr/bin/true -x a b");
    }

    #[test]
    fn test_output_is_captured_from_stdout() {
        struct EchoAction;
        impl MaintenanceAction for EchoAction {
            fn name(&self) -> &'static str {
                "echo-action"
            }
            fn category(&self) -> ActionCategory {
                ActionCategory::SystemState
            }
            fn description(&self) -> &'static str {
                "echoes a line"
            }
            fn risk(&self) -> RiskLevel {
                RiskLevel::Low
            }
            fn is_reversible(&self) -> bool {
                true
            }
            fn commands(&self) -> Vec<SystemCommand> {
                vec![SystemCommand::new(
                    "/bin/echo",
                    vec!["hello-world".to_string()],
                )]
            }
        }

        let outcome = EchoAction.execute().unwrap();
        assert_eq!(outcome.output, Some("hello-world".to_string()));
    }
}
