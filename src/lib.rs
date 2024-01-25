use std::fmt::Debug;
use std::path::Path;
use std::str::FromStr;

use arqoii::types::QoiHeader;

#[cfg(feature = "cli")]
use clap::{builder::PossibleValue, ValueEnum};

use image::ImageBuffer;
use image::Luma;
use qrcode::render::Pixel;
use qrcode::QrCode;

#[derive(Clone)]
#[non_exhaustive]
pub enum ImageFormat {
    #[non_exhaustive]
    ImageFormat(image::ImageFormat),
    #[cfg(feature = "qoi")]
    #[non_exhaustive]
    Qoi,
}

impl Debug for ImageFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ImageFormat::ImageFormat(format) => write!(f, "{format:?}"),
            ImageFormat::Qoi => write!(f, "Qoi"),
        }
    }
}

impl Default for ImageFormat {
    fn default() -> Self {
        Self::ImageFormat(image::ImageFormat::Png)
    }
}

#[cfg(feature = "cli")]
impl ValueEnum for ImageFormat {
    fn value_variants<'a>() -> &'a [Self] {
        &[
            Self::Qoi,
            Self::ImageFormat(image::ImageFormat::Png),
            Self::ImageFormat(image::ImageFormat::Jpeg),
        ]
    }

    fn to_possible_value(&self) -> Option<clap::builder::PossibleValue> {
        let name = format!("{self:?}").to_lowercase();
        Some(PossibleValue::new(name))
    }
}

impl ImageFormat {
    pub fn png() -> Self {
        Self::ImageFormat(image::ImageFormat::Png)
    }

    #[cfg(feature = "qoi")]
    pub fn qoi() -> Self {
        Self::Qoi
    }
}

struct Image {
    buffer: ImageBuffer<Luma<u8>, Vec<u8>>,
}

impl Image {
    pub fn save(&self, format: ImageFormat, file_path: &Path) -> Result<(), GenerationError> {
        match format {
            ImageFormat::ImageFormat(format) => {
                self.buffer.save_with_format(file_path, format)?;
            }
            ImageFormat::Qoi => {
                let data = arqoii::QoiEncoder::new(
                    QoiHeader::new(
                        self.buffer.width(),
                        self.buffer.height(),
                        arqoii::types::QoiChannels::Rgb,
                        arqoii::types::QoiColorSpace::SRgbWithLinearAlpha,
                    ),
                    self.buffer.pixels().map(|px| arqoii::Pixel {
                        r: px.0[0],
                        g: px.0[0],
                        b: px.0[0],
                        a: 255,
                    }),
                )
                .collect::<Vec<_>>();
                std::fs::write(file_path, data)?;
            }
        }
        Ok(())
    }
    pub fn save_guess_format(&self, file_path: &Path) -> Result<(), GenerationError> {
        if cfg!(feature = "qoi") && file_path.extension().is_some_and(|ext| ext == "qoi") {
            self.save(ImageFormat::Qoi, file_path)
        } else {
            self.buffer.save(file_path)?;
            Ok(())
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Px(Luma<u8>);

struct Canvas(Px, Image);

impl Pixel for Px {
    type Image = Image;

    type Canvas = Canvas;

    fn default_color(color: qrcode::Color) -> Self {
        Self(Luma([color.select(0, 255)]))
    }
}

impl qrcode::render::Canvas for Canvas {
    type Pixel = Px;

    type Image = <Px as Pixel>::Image;

    fn new(width: u32, height: u32, dark_pixel: Self::Pixel, light_pixel: Self::Pixel) -> Self {
        Self(
            dark_pixel,
            Image {
                buffer: ImageBuffer::from_pixel(width, height, light_pixel.0),
            },
        )
    }

    fn draw_dark_pixel(&mut self, x: u32, y: u32) {
        self.1.buffer.put_pixel(x, y, self.0 .0)
    }

    fn into_image(self) -> Self::Image {
        self.1
    }
}

#[derive(Debug, thiserror::Error)]
pub enum GenerationError {
    #[error("{0}")]
    QrError(#[from] qrcode::types::QrError),
    #[error("{0}")]
    ImageError(#[from] image::error::ImageError),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0:?}")]
    InvalidEpcCode(#[from] InvalidEpcCode),
}

#[derive(Debug, Clone)]
pub struct EpcQr {
    character_set: CharacterSet,
    /// AT-23 BIC of Beneficiary Bank (8/11 characters)
    /// Mandatory in Version 1
    /// Optional in Version 2 inside the EEA
    bic: Option<String>,
    /// AT-21 Name of Beneficiary (max. 70. characters)
    beneficiary_name: String,
    /// AT-20 Account # of Beneficiary (max 34. characters)
    /// Only IBAN is allowed
    beneficiary_account: String,
    // AT-04 Amount in Euro
    // Must be between 0.01 and 999999999.99 inclusive
    amount: Option<Amount>,
    /// AT-44 Purpose of Credit Transfer (max. 4 characters)
    purpose: Option<String>,
    remittance: Option<Remittance>,
    /// Beneficiary to originator Information (max. 70 characters)
    info: Option<String>,
}

impl EpcQr {
    const MAX_LENGTH_BYTES: usize = 331;

    pub fn new(beneficiary_name: String, beneficiary_account: String) -> Self {
        Self {
            character_set: CharacterSet::Utf8,
            bic: None,
            beneficiary_name,
            beneficiary_account,
            amount: None,
            purpose: None,
            remittance: None,
            info: None,
        }
    }

    pub fn with_bic(mut self, bic: Option<String>) -> Self {
        self.bic = bic;
        self
    }

    pub fn with_amount(mut self, amount: Option<Amount>) -> Self {
        self.amount = amount;
        self
    }

    pub fn with_purpose(mut self, purpose: Option<String>) -> Self {
        self.purpose = purpose;
        self
    }

    pub fn with_remittance(mut self, remittance: Option<Remittance>) -> Self {
        self.remittance = remittance;
        self
    }

    pub fn with_info(mut self, info: Option<String>) -> Self {
        self.info = info;
        self
    }

    fn validate(&self) -> Result<(), InvalidEpcCode> {
        let invalid_bic = self
            .bic
            .as_ref()
            .is_some_and(|bic| ![8, 11].contains(&bic.chars().count()));
        let invalid_name = !(1..=70).contains(&self.beneficiary_name.chars().count());
        let invalid_iban = !(1..=34).contains(&self.beneficiary_account.chars().count());
        let invalid_amount = self.amount.as_ref().is_some_and(|amount| {
            999999999 < amount.euro || 99 < amount.cent || (amount.euro == 0 && amount.cent == 0)
        });
        let invalid_purpose = self
            .purpose
            .as_ref()
            .is_some_and(|purpose| !(1..=4).contains(&purpose.chars().count()));
        let invalid_remittance =
            self.remittance
                .as_ref()
                .is_some_and(|remittance| match remittance {
                    Remittance::Reference(reference) => {
                        !(1..=35).contains(&reference.chars().count())
                    }
                    Remittance::Text(text) => !(1..=140).contains(&text.chars().count()),
                });
        let invalid_info = self
            .info
            .as_ref()
            .is_some_and(|info| !(1..=70).contains(&info.chars().count()));

        if invalid_bic
            || invalid_name
            || invalid_iban
            || invalid_amount
            || invalid_purpose
            || invalid_remittance
            || invalid_info
        {
            Err(InvalidEpcCode::InvalidFieldLength {
                invalid_bic,
                invalid_name,
                invalid_iban,
                invalid_amount,
                invalid_purpose,
                invalid_remittance,
                invalid_info,
            })
        } else {
            Ok(())
        }
    }

    fn data(&self) -> Result<Vec<u8>, InvalidEpcCode> {

        self.validate()?;

        // while the enum lists all character sets for now we just support UTF-8
        assert!(matches!(self.character_set, CharacterSet::Utf8));

        let data = self.to_string();

        if data.len() <= Self::MAX_LENGTH_BYTES {
            Ok(data.into_bytes())
        } else {
            Err(InvalidEpcCode::TooLargeTotal)
        }
    }

    pub fn generate_image_file(
        &self,
        format: Option<ImageFormat>,
        file_path: &Path,
    ) -> Result<(), GenerationError> {
        let code = QrCode::new(self.data()?)?;

        let image = code.render::<Px>().build();

        match format {
            Some(format) => image.save(format, file_path)?,
            None => image.save_guess_format(file_path)?,
        }

        Ok(())
    }
}

impl ToString for EpcQr {
    fn to_string(&self) -> String {
        let mut data = String::with_capacity(Self::MAX_LENGTH_BYTES);

        let version = if self.bic.is_some() {
            "001\n"
        } else {
            "002\n"
        };


        data.push_str("BCD\n");
        data.push_str(version);

        data.push_str("1\n");
        data.push_str("SCT\n");
        if let Some(bic) = &self.bic {
            data.push_str(bic)
        }
        data.push('\n');
        data.push_str(&self.beneficiary_name);
        data.push('\n');
        data.push_str(&self.beneficiary_account);

        if let Some(amount) = &self.amount {
            data.push('\n');
            let amount = if amount.cent % 10 == 0 {
                format!("{}.{}", amount.euro, amount.cent / 10)
            } else {
                format!("{}.{:02}", amount.euro, amount.cent)
            };
            data.push_str(&format!("EUR{amount}"));
        } else if self.purpose.is_some() || self.remittance.is_some() || self.info.is_some() {
            data.push('\n');
        }

        if let Some(purpose) = &self.purpose {
            data.push('\n');
            data.push_str(purpose);
        } else if self.remittance.is_some() || self.info.is_some() {
            data.push('\n');
        }

        if let Some(Remittance::Reference(rem) | Remittance::Text(rem)) = &self.remittance {
            data.push('\n');
            data.push_str(rem);
        } else if let Some(info) = &self.info {
            data.push('\n');
            data.push_str(info);
        }

        data
    }
}

#[derive(Debug, thiserror::Error)]
pub enum InvalidEpcCode {
    #[error("Total data is larger than the maximal allowed 331 bytes!")]
    TooLargeTotal,
    #[error("At most one remittance field (text/reference) may be specified!")]
    DuplicateRemittance,
    #[error("At least one field had an invalid length")]
    InvalidFieldLength {
        invalid_bic: bool,
        invalid_name: bool,
        invalid_iban: bool,
        invalid_amount: bool,
        invalid_purpose: bool,
        invalid_remittance: bool,
        invalid_info: bool,
    },
}

#[derive(Debug, Clone)]
pub struct Amount {
    // 0 <= euro <= 999999999
    euro: u32,
    // 0 <= cent < 100
    // unless euro is 0 then  0 < cent
    cent: u8,
}

#[derive(Debug, thiserror::Error)]
pub enum InvalidAmount {
    #[error("The amount must be between 0.01 and 999999999.99, but was {euro}.{cent:02}")]
    OutOfRange {
        euro: u32,
        cent: u8,
    },
    #[error("Failed to parse Amount: {0}")]
    ParseIntError(#[from] std::num::ParseIntError),
    #[error("Invalid format, expected #.##, but couldn't find '.'")]
    NoSeparator
}

impl FromStr for Amount {
    type Err = InvalidAmount;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (euro, cent) = s.split_once('.').ok_or(InvalidAmount::NoSeparator)?;
        let euro  = euro.parse()?;
        let cent = cent.parse()?;
        if 999999999 < euro || 99 < cent || (euro == 0 && cent == 0) {
            return Err(InvalidAmount::OutOfRange { euro, cent });
        }
        Ok(Self {euro, cent})
    }
}

#[derive(Debug, Clone)]
pub enum Remittance {
    /// AT-05 Remittance information (Structured/Reference)
    /// (max. 35 characters)
    Reference(String),
    /// AT-05 Remittance information (Unstructured/Text)
    /// (max. 140 characters)
    Text(String),
}

impl Remittance {
    pub fn text(&self) -> &str {
        let (Remittance::Reference(text) | Remittance::Text(text)) = self;
        text
    }
}

#[derive(Debug, Clone)]
pub enum CharacterSet {
    Utf8 = 1,
    ISO8859_01 = 2,
    ISO8859_02 = 3,
    ISO8859_04 = 4,
    ISO8859_05 = 5,
    ISO8859_07 = 6,
    ISO8859_10 = 7,
    ISO8859_15 = 8,
}
