//! Native formats and their independent streaming output models.
use super::{h264_format, hevc, media, vp9};
use ddi_library::h264::{OutputOrder, PictureOrder};
use ddi_platform::video::YuvMatrix;

pub(super) enum Format {
    H264(h264_format::Format),
    Hevc(hevc::Format),
    Vp9(vp9::Format),
}
impl Format {
    pub fn codec(&self) -> &'static str {
        match self {
            Self::H264(_) => "h264",
            Self::Hevc(_) => "hevc",
            Self::Vp9(_) => "vp9",
        }
    }
    pub fn width(&self) -> u32 {
        match self {
            Self::H264(f) => f.stream.sps.width,
            Self::Hevc(f) => f.width,
            Self::Vp9(f) => f.width,
        }
    }
    pub fn height(&self) -> u32 {
        match self {
            Self::H264(f) => f.stream.sps.height,
            Self::Hevc(f) => f.height,
            Self::Vp9(f) => f.height,
        }
    }
    pub fn delay(&self) -> u32 {
        match self {
            Self::H264(f) => f.delay,
            Self::Hevc(f) => f.delay,
            Self::Vp9(_) => 0,
        }
    }
    pub fn full_range(&self) -> bool {
        match self {
            Self::H264(f) => f.stream.sps.full_range,
            Self::Hevc(f) => f.full_range,
            Self::Vp9(f) => f.full_range,
        }
    }
    pub fn matrix(&self) -> YuvMatrix {
        let bt709 = match self {
            Self::H264(f) => f.stream.sps.bt709(),
            Self::Hevc(f) => f.matrix == Some(1),
            Self::Vp9(f) => f.bt709,
        };
        if bt709 {
            YuvMatrix::Bt709
        } else {
            YuvMatrix::Bt601
        }
    }
    pub fn order(&self) -> Order {
        match self {
            Self::H264(f) => Order::H264 {
                pictures: PictureOrder::new(&f.stream),
                output: OutputOrder::new(f.delay),
            },
            Self::Hevc(_) => Order::Hevc(hevc::Order::default()),
            Self::Vp9(_) => Order::Vp9,
        }
    }
}
pub(super) enum Order {
    H264 {
        pictures: PictureOrder,
        output: OutputOrder,
    },
    Hevc(hevc::Order),
    Vp9,
}
impl Order {
    pub fn submit(
        &mut self,
        f: &Format,
        bytes: &[u8],
        id: usize,
        packet_path: bool,
    ) -> Result<(Vec<u8>, Vec<usize>), String> {
        match (self, f) {
            (Self::H264 { pictures, output }, Format::H264(f)) => {
                if packet_path {
                    media::checked_picture(bytes, f.stream.framing)?;
                }
                let picture = pictures
                    .picture(bytes, f.stream.framing, &f.stream.sps)
                    .ok_or("H.264 packet has no supported picture")?;
                if id == 0 && !picture.idr {
                    return Err("H.264 seek did not land on an IDR".into());
                }
                if picture.b && f.delay == 0 {
                    return Err("H.264 B picture violates the fixed reorder delay".into());
                }
                let released = output.push(id, picture, id as u32);
                if released.dropped.is_some() {
                    return Err("H.264 output order dropped a picture".into());
                }
                Ok((
                    f.sample(bytes)?,
                    released.out.map(|(id, _)| id).into_iter().collect(),
                ))
            }
            (Self::Hevc(order), Format::Hevc(f)) => {
                let (sample, picture) = f.sample(bytes)?;
                Ok((sample, order.push(f, picture, id)?))
            }
            (Self::Vp9, Format::Vp9(f)) => {
                f.check(bytes, id == 0)?;
                Ok((bytes.to_vec(), vec![id]))
            }
            _ => Err("native codec changed".into()),
        }
    }
    pub fn drain(&mut self) -> Vec<usize> {
        match self {
            Self::H264 { output, .. } => output.drain().into_iter().map(|(id, _)| id).collect(),
            Self::Hevc(o) => o.drain(),
            Self::Vp9 => Vec::new(),
        }
    }
}
