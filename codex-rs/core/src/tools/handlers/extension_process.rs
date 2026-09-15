//! Host authority for bounded extension subprocesses. Product crates submit
//! argv; Codex owns policy, environment, sandbox construction and cancellation.

use std::future::Future;
use std::pin::Pin;
use std::sync::Weak;
use std::time::Duration;

use codex_protocol::models::AdditionalPermissionProfile;
use codex_sandboxing::SandboxCommand;
use codex_sandboxing::SandboxablePreference;
use codex_tools::ToolProcessExecutor;
use codex_tools::ToolProcessOutput;
use codex_tools::ToolProcessRequest;

use crate::exec::ExecCapturePolicy;
use crate::exec::ExecExpiration;
use crate::exec::execute_bounded_request;
use crate::exec_env::create_env;
use crate::exec_env::inject_permission_profile_env;
use crate::exec_env::inject_session_env;
use crate::exec_policy::ExecApprovalRequest;
use crate::sandboxing::ExecOptions;
use crate::sandboxing::SandboxPermissions;
use crate::session::turn_context::TurnEnvironment;
use crate::tools::context::ToolInvocation;
use crate::tools::handlers::apply_granted_turn_permissions;
use crate::tools::orchestrator::ToolOrchestrator;
use crate::tools::sandboxing::Approvable;
use crate::tools::sandboxing::ApprovalAction;
use crate::tools::sandboxing::ExecApprovalRequirement;
use crate::tools::sandboxing::SandboxAttempt;
use crate::tools::sandboxing::Sandboxable;
use crate::tools::sandboxing::ToolCtx;
use crate::tools::sandboxing::ToolError;
use crate::tools::sandboxing::ToolRuntime;

// Retaining an extension call must not retain a finished session or its authority.
pub(super) struct CoreProcessExecutor(pub(super) Weak<ToolInvocation>);

impl ToolProcessExecutor for CoreProcessExecutor {
    fn check_available(&self, environment_id: &str) -> Result<(), String> {
        let invocation = self.0.upgrade().ok_or("tool invocation has ended")?;
        if invocation.cancellation_token.is_cancelled() {
            return Err("tool invocation cancelled".into());
        }
        let environment = invocation
            .step_context
            .environments
            .turn_environments()
            .find(|environment| environment.selection.environment_id == environment_id)
            .ok_or("unknown execution environment")?;
        if !cfg!(any(target_os = "linux", target_os = "macos"))
            || environment.environment.is_remote()
        {
            return Err(
                "bounded extension execution requires a local Linux or macOS environment".into(),
            );
        }
        if invocation.turn.network.is_some() || environment.config().network_policy.is_some() {
            return Err(
                "bounded extension execution does not yet support managed network proxies".into(),
            );
        }
        Ok(())
    }

    fn execute<'a>(
        &'a self,
        request: ToolProcessRequest,
    ) -> Pin<Box<dyn Future<Output = Result<ToolProcessOutput, String>> + Send + 'a>> {
        Box::pin(async move {
            self.check_available(&request.environment_id)?;
            if request.command.is_empty()
                || request.timeout_ms == 0
                || request.timeout_ms > 120_000
                || request.max_output_bytes > 4096
            {
                return Err("invalid bounded process request".into());
            }
            let invocation = self.0.upgrade().ok_or("tool invocation has ended")?;
            let environment = invocation
                .step_context
                .environments
                .turn_environments()
                .find(|environment| environment.selection.environment_id == request.environment_id)
                .ok_or("unknown execution environment")?
                .clone();
            let additional_permissions = apply_granted_turn_permissions(
                invocation.session.as_ref(),
                &environment,
                &request.cwd,
                SandboxPermissions::UseDefault,
                /*additional_permissions*/ None,
            )
            .await
            .additional_permissions;
            let approval = invocation
                .session
                .services
                .exec_policy
                .create_exec_approval_requirement_for_command(ExecApprovalRequest {
                    command: &request.command,
                    approval_policy: invocation.step_context.settings.approval_policy(),
                    permission_profile: environment.permission_profile().clone(),
                    environment_policy: environment.config().exec_policy.as_ref(),
                    windows_sandbox_level: environment.config().windows_sandbox_level,
                    sandbox_permissions: SandboxPermissions::UseDefault,
                    prefix_rule: None,
                    allow_prefix_rules: invocation.turn.allow_prefix_rules(),
                })
                .await;
            let context = ToolCtx {
                session: invocation.session.clone(),
                step_context: invocation.step_context.clone(),
                cancellation_token: invocation.cancellation_token.clone(),
                call_id: invocation.call_id.clone(),
                tool_name: invocation.tool_name.clone(),
            };
            let request = ProcessRequest {
                process: request,
                environment,
                approval,
                additional_permissions,
            };
            let result = ToolOrchestrator::new()
                .run(&mut ProcessRuntime, &request, &context)
                .await
                .map_err(|error| format!("{error:?}"))?;
            Ok(result.output)
        })
    }
}

struct ProcessRequest {
    process: ToolProcessRequest,
    environment: TurnEnvironment,
    approval: ExecApprovalRequirement,
    additional_permissions: Option<AdditionalPermissionProfile>,
}

struct ProcessRuntime;

impl Sandboxable for ProcessRuntime {
    fn sandbox_preference(&self) -> SandboxablePreference {
        SandboxablePreference::Auto
    }
    // Re-executing a whole program could duplicate already completed side effects.
    fn escalate_on_failure(&self) -> bool {
        false
    }
}

impl Approvable<ProcessRequest> for ProcessRuntime {
    fn exec_approval_requirement(&self, req: &ProcessRequest) -> Option<ExecApprovalRequirement> {
        Some(req.approval.clone())
    }

    fn approval_action(
        &self,
        req: &ProcessRequest,
        call_id: &str,
    ) -> std::io::Result<ApprovalAction> {
        Ok(ApprovalAction::ExecCommand {
            id: call_id.to_owned(),
            environment_id: req.process.environment_id.clone(),
            command: req.process.command.clone(),
            hook_command: shlex::try_join(req.process.command.iter().map(String::as_str))
                .map_err(std::io::Error::other)?,
            cwd: req.process.cwd.clone(),
            sandbox_permissions: SandboxPermissions::UseDefault,
            additional_permissions: req.additional_permissions.clone(),
            justification: Some(req.process.description.clone()),
            tty: false,
            proposed_execpolicy_amendment: None,
        })
    }
}

impl ToolRuntime<ProcessRequest, ToolProcessOutput> for ProcessRuntime {
    fn turn_environment<'a>(&self, req: &'a ProcessRequest) -> &'a TurnEnvironment {
        &req.environment
    }

    #[tracing::instrument(level = "info", skip_all, fields(tool = %ctx.tool_name, call_id = %ctx.call_id, phase = %req.process.description))]
    async fn run(
        &mut self,
        req: &ProcessRequest,
        attempt: &SandboxAttempt<'_>,
        ctx: &ToolCtx,
    ) -> Result<ToolProcessOutput, ToolError> {
        if ctx.cancellation_token.is_cancelled() {
            return Err(ToolError::Rejected(
                "tool invocation cancelled before spawn".into(),
            ));
        }
        let mut env = create_env(
            req.environment.shell_environment_policy(),
            Some(ctx.session.thread_id),
        );
        env.extend(req.process.env_overrides.clone());
        inject_session_env(&mut env, ctx.session.session_id());
        inject_permission_profile_env(
            &mut env,
            req.environment.active_permission_profile().as_ref(),
        );
        let command = SandboxCommand {
            program: req.process.command[0].clone().into(),
            args: req.process.command[1..].to_vec(),
            cwd: req.process.cwd.clone(),
            env,
            managed_network: None,
            additional_permissions: req.additional_permissions.clone(),
        };
        let options = ExecOptions {
            expiration: ExecExpiration::TimeoutOrCancellation {
                timeout: Duration::from_millis(req.process.timeout_ms),
                cancellation: ctx.cancellation_token.clone(),
            },
            capture_policy: ExecCapturePolicy::BoundedProcess {
                max_bytes: req.process.max_output_bytes,
            },
        };
        let exec = attempt
            .env_for(
                command,
                options,
                /*network*/ None,
                Some(&req.process.environment_id),
            )
            .map_err(ToolError::Codex)?;
        execute_bounded_request(exec, req.process.max_output_bytes)
            .await
            .map_err(ToolError::Codex)
    }
}
