use serde::Serialize;

#[derive(Serialize)]
pub struct SuccessResponse<T> {
    pub message: String,
    pub data: T,
}
