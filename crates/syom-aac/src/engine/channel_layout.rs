//! Minimal ChannelLayout / ChannelPosition subset for AAC multichannel reorder.
//! Vendored from oxideav-core (MIT), stripped of unrelated media types.

#![allow(missing_docs)]

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChannelPosition {
    FrontLeft,
    FrontRight,
    FrontCenter,
    LowFrequency,
    BackLeft,
    BackRight,
    FrontLeftOfCenter,
    FrontRightOfCenter,
    BackCenter,
    SideLeft,
    SideRight,
    TopFrontLeft,
    TopFrontRight,
    TopBackLeft,
    TopBackRight,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChannelLayout {
    Mono,
    Stereo,
    Stereo21,
    Surround30,
    Quad,
    Surround40,
    Surround41,
    Surround50,
    Surround51,
    Surround60,
    Surround61,
    Surround70,
    Surround71,
    LoRo,
    LtRt,
    DiscreteN(u16),
}

impl ChannelLayout {
    pub fn channel_count(&self) -> u16 {
        match self {
            Self::Mono => 1,
            Self::Stereo | Self::LoRo | Self::LtRt => 2,
            Self::Stereo21 | Self::Surround30 => 3,
            Self::Quad | Self::Surround40 => 4,
            Self::Surround41 | Self::Surround50 => 5,
            Self::Surround51 | Self::Surround60 => 6,
            Self::Surround61 | Self::Surround70 => 7,
            Self::Surround71 => 8,
            Self::DiscreteN(n) => *n,
        }
    }

    pub fn positions(&self) -> &'static [ChannelPosition] {
        use ChannelPosition::*;
        match self {
            Self::Mono => &[FrontCenter],
            Self::Stereo | Self::LoRo | Self::LtRt => &[FrontLeft, FrontRight],
            Self::Stereo21 => &[FrontLeft, FrontRight, LowFrequency],
            Self::Surround30 => &[FrontLeft, FrontRight, FrontCenter],
            Self::Quad => &[FrontLeft, FrontRight, SideLeft, SideRight],
            Self::Surround40 => &[FrontLeft, FrontRight, FrontCenter, BackCenter],
            Self::Surround41 => &[FrontLeft, FrontRight, FrontCenter, BackCenter, LowFrequency],
            Self::Surround50 => &[FrontLeft, FrontRight, FrontCenter, SideLeft, SideRight],
            Self::Surround51 => &[
                FrontLeft,
                FrontRight,
                FrontCenter,
                LowFrequency,
                SideLeft,
                SideRight,
            ],
            Self::Surround60 => &[
                FrontLeft,
                FrontRight,
                FrontCenter,
                BackCenter,
                SideLeft,
                SideRight,
            ],
            Self::Surround61 => &[
                FrontLeft,
                FrontRight,
                FrontCenter,
                LowFrequency,
                BackCenter,
                SideLeft,
                SideRight,
            ],
            Self::Surround70 => &[
                FrontLeft,
                FrontRight,
                FrontCenter,
                SideLeft,
                SideRight,
                BackLeft,
                BackRight,
            ],
            Self::Surround71 => &[
                FrontLeft,
                FrontRight,
                FrontCenter,
                LowFrequency,
                SideLeft,
                SideRight,
                BackLeft,
                BackRight,
            ],
            Self::DiscreteN(_) => &[],
        }
    }
}
