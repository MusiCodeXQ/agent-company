UPDATE runtime_profiles
SET tool_policy = '{"workspace":true,"bash":true,"hostEnvironment":true,"personalToolchains":true,"softwareSuggestions":true}',
    config = '{"maxIterations":20,"toolTimeoutSeconds":120}'
WHERE id = 'runtime-profile-adk-rust-01';
