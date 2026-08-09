// ---- Port: ModelGateway — interface for LLM model access ----
//
// The model gateway abstracts the LLM provider. Currently this is
// implemented by piko_llmd::gateway::InferenceGateway.

pub use piko_llmd::gateway::InferenceGateway;
