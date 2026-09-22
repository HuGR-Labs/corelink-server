"""Cache-only execution platform for the CoreLink Buck2 REAPI bridge."""

def _corelink_cache_platforms_impl(ctx):
    configuration = ConfigurationInfo(constraints = {}, values = {})
    platform = ExecutionPlatformInfo(
        label = ctx.label.raw_target(),
        configuration = configuration,
        executor_config = CommandExecutorConfig(
            local_enabled = True,
            remote_enabled = False,
            remote_cache_enabled = True,
            use_limited_hybrid = False,
            remote_execution_action_key = read_config("corelink", "provenance"),
            remote_execution_use_case = "corelink-cache-only",
        ),
    )
    return [
        DefaultInfo(),
        ExecutionPlatformRegistrationInfo(platforms = [platform], fallback = "error"),
    ]

corelink_cache_platforms = rule(impl = _corelink_cache_platforms_impl, attrs = {})
