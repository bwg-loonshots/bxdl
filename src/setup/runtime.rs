//! Workflow entry points share the manager's authenticated executor contract.
//! The workflow keeps its own approval journal; the package owns node attempts.
use crate::{
    engine::instance::{
        self,
        executor::{self, Request, ServiceAction},
    },
    error::{BxdlError, Result},
};
use std::{path::Path, time::Duration};

pub(super) fn initialize(
    instance: &Path,
    resume: bool,
    timeout: Duration,
) -> Result<instance::Summary> {
    match executor::try_execute(
        instance,
        &Request::Initialize { resume, timeout },
        || Ok(()),
    )? {
        Some(response) => response.into_initialization(),
        None => instance::initialize(instance, resume, timeout),
    }
}

pub(super) fn start(
    instance: &Path,
    timeout: Duration,
) -> Result<instance::service::ServiceSummary> {
    match executor::try_execute(
        instance,
        &Request::Service {
            action: ServiceAction::Start,
            wait: false,
            timeout,
        },
        || Ok(()),
    )? {
        Some(response) => response.into_service(),
        None => instance::service::start(instance, timeout),
    }
}

pub(super) fn status(
    instance: &Path,
    timeout: Duration,
) -> Result<instance::service::ServiceSummary> {
    match executor::try_execute(
        instance,
        &Request::Service {
            action: ServiceAction::Status,
            wait: false,
            timeout,
        },
        || Ok(()),
    )? {
        Some(response) => response.into_service(),
        None => instance::service::status(instance, timeout),
    }
}

pub(super) fn wait_ready_for(
    instance: &Path,
    expected: &str,
    timeout: Duration,
) -> Result<instance::service::ServiceSummary> {
    // The legacy executor's --wait observes its own initial attempt. Check both
    // sides so the workflow never accepts another attempt as the approved one.
    let before = status(instance, Duration::from_secs(15))?;
    if before.attempt_id.as_deref() != Some(expected) {
        return Err(changed());
    }
    match executor::try_execute(
        instance,
        &Request::Service {
            action: ServiceAction::Status,
            wait: true,
            timeout,
        },
        || Ok(()),
    )? {
        Some(response) => {
            let value = response.into_service()?;
            if value.attempt_id.as_deref() != Some(expected) {
                return Err(changed());
            }
            Ok(value)
        }
        None => instance::service::wait_ready_for(instance, expected, timeout),
    }
}
fn changed() -> BxdlError {
    BxdlError::new(
        "SERVICE_ATTEMPT_CHANGED",
        "관측 대상 실행 시도가 변경됐습니다. 기존 승인을 다른 시도로 옮기지 않았습니다.",
    )
}
