#![warn(clippy::cargo)]

use clap::Parser;
use epc_qr_code_generator::{Amount, EpcQr, GenerationError, ImageFormat, InvalidEpcCode, Remittance};

#[derive(Debug, clap::Parser)]
struct CliArgs {
    #[arg(long, short)]
    bic: Option<String>,
    beneficiary_name: String,
    beneficiary_account: String,
    #[arg(long, short)]
    amount: Option<Amount>,
    #[arg(long, short)]
    purpose : Option<String>,
    #[arg(long = "reference", short = 'r')]
    remittance_reference: Option<String>,
    #[arg(long = "text", short = 't')]
    remittance_text: Option<String>,
    #[arg(long, short)]
    info: Option<String>,
    #[arg(long, default_value_t, value_enum)]
    image_format: ImageFormat,
}

fn main() -> Result<(), GenerationError> {
    let mut args = CliArgs::parse();

    let remittance = match (args.remittance_reference, args.remittance_text) {
        (None, Some(text)) => Some(Remittance::Text(text)),
        (Some(reference), None) => Some(Remittance::Reference(reference)),
        (None, None) => None,
        (Some(_), Some(_)) => {
            return Err(GenerationError::InvalidEpcCode(
                InvalidEpcCode::DuplicateRemittance,
            ))
        }
    };

    let mut file_name = match (&args.bic, &remittance) {
        (None, None) => {
            format!("epc-{}-qr-code.png", args.beneficiary_account)
        }
        (None, Some(remittance)) => {
            format!(
                "epc-{}-{}-qr-code.png",
                args.beneficiary_account,
                remittance.text()
            )
        }
        (Some(bic), None) => {
            format!("epc-{bic}-{}-qr-code.png", args.beneficiary_account)
        }
        (Some(bic), Some(remittance)) => {
            format!(
                "epc-{bic}-{}-{}-qr-code.png",
                args.beneficiary_account,
                remittance.text()
            )
        }
    };

    file_name = file_name.replace(['/', '\\', ' '], "_");

    args.beneficiary_account = args.beneficiary_account.replace(' ', "");

    let epc_qr = EpcQr::new(args.beneficiary_name, args.beneficiary_account)
        .with_bic(args.bic)
        .with_amount(args.amount)
        .with_purpose(args.purpose)
        .with_remittance(remittance)
        .with_info(args.info);

    let epc_qr_string = epc_qr.to_string();
    println!("{epc_qr_string}");

    epc_qr.generate_image_file(Some(args.image_format), file_name.as_ref())?;

    Ok(())
}
