use embedded_error_chain::ErrorCategory;
use mqtt_format::v5::packets::disconnect::DisconnectReasonCode;

#[derive(Clone, Copy, ErrorCategory)]
#[repr(u8)]
pub enum DistributorError {
    #[error("Topic too long")]
    TopicTooLong,
    #[error("Message too long")]
    MessageTooLong,
    #[error("Queue full")]
    QueueFull,
    #[error("Unexpected packet")]
    UnexpectedPacket,
    #[error("Unknown error")]
    Unknown,
}

impl From<TopicsError> for DistributorError {
    fn from(e: TopicsError) -> Self {
        match e {
            TopicsError::TopicTooLong => DistributorError::TopicTooLong,
            TopicsError::Full => DistributorError::QueueFull,
        }
    }
}

impl From<DistributorError> for DisconnectReasonCode {
    fn from(e: DistributorError) -> Self {
        match e {
            DistributorError::TopicTooLong => DisconnectReasonCode::TopicNameInvalid,
            DistributorError::MessageTooLong => DisconnectReasonCode::PacketTooLarge,
            DistributorError::QueueFull => DisconnectReasonCode::ReceiveMaximumExceeded,
            DistributorError::UnexpectedPacket => DisconnectReasonCode::ProtocolError,
            DistributorError::Unknown => DisconnectReasonCode::UnspecifiedError,
        }
    }
}

#[derive(Clone, Copy, ErrorCategory)]
#[repr(u8)]
pub enum MqttCodecError {
    Incomplete,
    Invalid,
    InvalidLength,
    BufferTooSmall,
    ConnectionReset,
}

#[derive(Clone, Copy, ErrorCategory)]
#[repr(u8)]
pub enum TopicsError {
    TopicTooLong,
    Full,
}
