//! Definition-driven runtime capability validation.

use crate::domain::AgentLaunchRequest;

use super::RuntimeError;

/// Validate static request support without probing, installing, or creating files.
pub fn validate_launch_request(request: &AgentLaunchRequest) -> Result<(), RuntimeError> {
    let definition = crate::domain::agent_definition::AgentDefinition::shipped()
        .into_iter()
        .find(|definition| definition.id == request.type_id)
        .ok_or_else(|| {
            RuntimeError::SpawnFailed(format!("unknown active agent type {}", request.type_id))
        })?;
    definition
        .validate()
        .map_err(|error| RuntimeError::SpawnFailed(error.to_string()))?;
    if definition
        .operations
        .support_for(request.operation)
        .supported
        .is_unsupported()
    {
        return Err(RuntimeError::SpawnFailed(format!(
            "{} does not support {:?}",
            definition.display_name, request.operation
        )));
    }
    super::launch_compose::launch_signature_from_request(request).map(|_| ())
}
