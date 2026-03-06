use super::*;

/// Execution context passed to tool handlers.
pub struct ToolExecutionContext<'a> {
    pub worktree: &'a Path,
    pub scope: Option<&'a Scope>,
    pub sandbox_policy: NativeSandboxPolicy,
    pub approval_policy: NativeApprovalPolicy,
    pub network_policy: NativeNetworkPolicy,
    pub command_policy: NativeCommandPolicy,
    pub exec_policy_manager: NativeExecPolicyManager,
    pub approval_cache: RefCell<NativeApprovalCache>,
    pub network_approval_cache: RefCell<NativeNetworkApprovalCache>,
    pub env: &'a HashMap<String, String>,
}
