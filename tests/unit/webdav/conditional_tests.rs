use fast_dav_rs::webdav::conditional_write_error;
use fast_dav_rs::{Error, Operation};
use hyper::{Response, StatusCode};

fn response(status: u16) -> Response<bytes::Bytes> {
    Response::builder()
        .status(StatusCode::from_u16(status).unwrap())
        .body(bytes::Bytes::new())
        .unwrap()
}

#[test]
fn conditional_write_error_accepts_2xx() {
    assert!(
        conditional_write_error(Operation::PutIfMatch, &response(200)).is_ok(),
        "200 OK on a conditional write classifies as success"
    );
    assert!(
        conditional_write_error(Operation::PutIfMatch, &response(204)).is_ok(),
        "204 No Content (typical PUT/DELETE success) classifies as success"
    );
}

#[test]
fn conditional_write_error_maps_412_to_precondition_failed() {
    let err = conditional_write_error(Operation::PutIfMatch, &response(412)).unwrap_err();
    match &err {
        Error::PreconditionFailed { operation, .. } => {
            assert_eq!(
                *operation,
                Operation::PutIfMatch,
                "the failed conditional write must be carried by the error"
            );
        }
        other => panic!("expected PreconditionFailed for 412, got: {other:?}"),
    }
    let display = err.to_string();
    assert!(
        display.contains("412"),
        "the Display must name the 412 status: {display}"
    );
    assert!(
        display.contains("PUT If-Match"),
        "the Display must carry the operation context: {display}"
    );
}

#[test]
fn conditional_write_error_maps_428_to_precondition_required() {
    let err = conditional_write_error(Operation::DeleteIfMatch, &response(428)).unwrap_err();
    match &err {
        Error::PreconditionRequired { operation, .. } => {
            assert_eq!(
                *operation,
                Operation::DeleteIfMatch,
                "the failed conditional write must be carried by the error"
            );
        }
        other => panic!("expected PreconditionRequired for 428, got: {other:?}"),
    }
    let display = err.to_string();
    assert!(
        display.contains("428"),
        "the Display must name the 428 status: {display}"
    );
    assert!(
        display.contains("DELETE If-Match"),
        "the Display must carry the operation context: {display}"
    );
}

#[test]
fn conditional_write_error_maps_other_statuses_to_unexpected_status() {
    for status in [400_u16, 403, 423, 500] {
        let err = conditional_write_error(Operation::PutIfMatch, &response(status)).unwrap_err();
        match &err {
            Error::UnexpectedStatus {
                operation,
                status: s,
                ..
            } => {
                assert_eq!(*operation, Operation::PutIfMatch);
                assert_eq!(*s, StatusCode::from_u16(status).unwrap());
            }
            other => panic!("expected UnexpectedStatus for {status}, got: {other:?}"),
        }
    }
}

#[test]
fn put_if_match_and_delete_if_match_operations_render() {
    assert_eq!(Operation::PutIfMatch.to_string(), "PUT If-Match");
    assert_eq!(Operation::DeleteIfMatch.to_string(), "DELETE If-Match");
}
