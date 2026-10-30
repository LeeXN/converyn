//! 纯领域规则与确定性状态机。此 crate 不依赖运行时、场景插件、数据库或网络客户端。
pub mod model;
use model::{domain, execution, health, host, package, service};
pub mod orchestration;
use orchestration::workflow;
use orchestration::{administration, controlplane, drift, history, machine, preparation};
pub mod identities;
use identities::{certificate, identity, registration};
use model::receipt as runner;
use model::{artifact as source, evidence as native, logs as task_log, variables as secrets};
