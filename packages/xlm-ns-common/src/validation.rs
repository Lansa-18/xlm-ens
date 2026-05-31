use crate::constants::{
    MAX_NAME_LENGTH, MAX_REGISTRATION_YEARS, MIN_NAME_LENGTH, MIN_REGISTRATION_YEARS,
};
use crate::errors::CommonError;
use crate::types::Tld;

pub fn validate_label(label: &str) -> Result<(), CommonError> {
    let len = label.len();

    if len < MIN_NAME_LENGTH {
        return Err(CommonError::NameTooShort);
    }

    if len > MAX_NAME_LENGTH {
        return Err(CommonError::NameTooLong);
    }

    if !label
        .chars()
        .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-')
    {
        return Err(CommonError::InvalidCharacters);
    }

    if label.starts_with('-') || label.ends_with('-') {
        return Err(CommonError::InvalidLabelBoundary);
    }

    Ok(())
}

pub fn validate_owner(owner: &str) -> Result<(), CommonError> {
    if owner.trim().is_empty() {
        return Err(CommonError::EmptyOwner);
    }

    Ok(())
}

pub fn validate_registration_years(years: u64) -> Result<(), CommonError> {
    if !(MIN_REGISTRATION_YEARS..=MAX_REGISTRATION_YEARS).contains(&years) {
        return Err(CommonError::InvalidRegistrationPeriod);
    }

    Ok(())
}

pub fn parse_fqdn(name: &str) -> Result<(String, Tld), CommonError> {
    let mut parts = name.split('.');
    let label = parts.next().ok_or(CommonError::InvalidName)?;
    let tld = parts.next().ok_or(CommonError::MissingTld)?;

    if parts.next().is_some() {
        return Err(CommonError::InvalidName);
    }

    validate_label(label)?;
    let parsed_tld = Tld::parse(tld).ok_or(CommonError::UnsupportedTld)?;

    Ok((label.to_string(), parsed_tld))
}

pub fn validate_chain_name(chain: &str) -> Result<(), CommonError> {
    if chain.trim().is_empty() {
        return Err(CommonError::EmptyChainName);
    }

    Ok(())
}

pub fn validate_contract_id(contract_id: &str) -> Result<(), CommonError> {
    let trimmed = contract_id.trim();
    if trimmed.is_empty() {
        return Err(CommonError::EmptyContractId);
    }

    if trimmed.len() != 56
        || !trimmed.starts_with('C')
        || !trimmed.chars().all(|ch| ch.is_ascii_alphanumeric())
    {
        return Err(CommonError::InvalidContractId);
    }

    Ok(())
}

pub fn validate_account_address(address: &str) -> Result<(), CommonError> {
    let trimmed = address.trim();
    if trimmed.is_empty() {
        return Err(CommonError::EmptyAccountAddress);
    }

    if trimmed.len() != 56
        || !trimmed.starts_with('G')
        || !trimmed.chars().all(|ch| ch.is_ascii_alphanumeric())
    {
        return Err(CommonError::InvalidAccountAddress);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        parse_fqdn, validate_account_address, validate_chain_name, validate_contract_id,
        validate_label, validate_registration_years,
    };
    use crate::constants::{
        MAX_NAME_LENGTH, MAX_REGISTRATION_YEARS, MIN_NAME_LENGTH, MIN_REGISTRATION_YEARS,
    };
    use crate::errors::CommonError;
    use crate::types::Tld;
    use proptest::prelude::*;

    #[test]
    fn rejects_short_labels() {
        let short_label = "a".repeat(MIN_NAME_LENGTH - 1);

        assert_eq!(validate_label(&short_label), Err(CommonError::NameTooShort));
    }

    #[test]
    fn rejects_long_labels() {
        let long_label = "a".repeat(MAX_NAME_LENGTH + 1);

        assert_eq!(validate_label(&long_label), Err(CommonError::NameTooLong));
    }

    #[test]
    fn rejects_unsupported_tlds() {
        assert_eq!(parse_fqdn("valid.eth"), Err(CommonError::UnsupportedTld));
    }

    #[test]
    fn rejects_invalid_characters() {
        assert_eq!(validate_label("ab_"), Err(CommonError::InvalidCharacters));
        assert_eq!(parse_fqdn("ab_.xlm"), Err(CommonError::InvalidCharacters));
    }

    #[test]
    fn rejects_labels_with_invalid_hyphen_boundaries() {
        assert_eq!(
            validate_label("-abc"),
            Err(CommonError::InvalidLabelBoundary)
        );
        assert_eq!(
            validate_label("abc-"),
            Err(CommonError::InvalidLabelBoundary)
        );
    }

    #[test]
    fn accepts_valid_fqdn() {
        assert_eq!(
            parse_fqdn("abc-123.xlm"),
            Ok(("abc-123".to_string(), Tld::Xlm))
        );
    }

    #[test]
    fn enforces_registration_year_bounds() {
        assert_eq!(validate_registration_years(MIN_REGISTRATION_YEARS), Ok(()));
        assert_eq!(validate_registration_years(MAX_REGISTRATION_YEARS), Ok(()));
        assert_eq!(
            validate_registration_years(MIN_REGISTRATION_YEARS - 1),
            Err(CommonError::InvalidRegistrationPeriod)
        );
        assert_eq!(
            validate_registration_years(MAX_REGISTRATION_YEARS + 1),
            Err(CommonError::InvalidRegistrationPeriod)
        );
    }

    #[test]
    fn validates_chain_name_presence() {
        assert_eq!(validate_chain_name("stellar"), Ok(()));
        assert_eq!(validate_chain_name("   "), Err(CommonError::EmptyChainName));
    }

    #[test]
    fn validates_contract_ids_and_account_addresses() {
        assert_eq!(
            validate_contract_id("C".repeat(56).as_str()),
            Ok(())
        );
        assert_eq!(
            validate_account_address("G".repeat(56).as_str()),
            Ok(())
        );
        assert_eq!(
            validate_contract_id("bad"),
            Err(CommonError::InvalidContractId)
        );
        assert_eq!(
            validate_account_address("bad"),
            Err(CommonError::InvalidAccountAddress)
        );
    }

    // ==========================================
    // Property-based Tests (Fuzzing)
    //
    // To run these property tests, use the standard cargo test command:
    //   cargo test -p xlm-ns-common
    //
    // Proptest will automatically generate thousands of random string
    // inputs to verify that the validation functions never panic and
    // correctly accept or reject edge cases.
    // ==========================================
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(1000))]

        #[test]
        fn doesnt_crash_on_random_input(s in "\\PC*") {
            let _ = validate_label(&s);
            let _ = parse_fqdn(&s);
        }

        #[test]
        fn accepts_strictly_valid_labels(s in "[a-z0-9]([a-z0-9-]*[a-z0-9])?") {
            if s.len() >= MIN_NAME_LENGTH && s.len() <= MAX_NAME_LENGTH {
                prop_assert_eq!(validate_label(&s), Ok(()));
            } else if s.len() < MIN_NAME_LENGTH {
                prop_assert_eq!(validate_label(&s), Err(CommonError::NameTooShort));
            } else {
                prop_assert_eq!(validate_label(&s), Err(CommonError::NameTooLong));
            }
        }

        #[test]
        fn rejects_labels_with_uppercase(s in "[a-zA-Z0-9-]*[A-Z][a-zA-Z0-9-]*") {
            if s.len() >= MIN_NAME_LENGTH && s.len() <= MAX_NAME_LENGTH {
                prop_assert_eq!(validate_label(&s), Err(CommonError::InvalidCharacters));
            }
        }

        #[test]
        fn rejects_fqdn_without_tld(label in "[a-z0-9]([a-z0-9-]*[a-z0-9])?") {
            prop_assert_eq!(parse_fqdn(&label), Err(CommonError::MissingTld));
        }

        #[test]
        fn parses_valid_fqdn(label in "[a-z0-9]([a-z0-9-]*[a-z0-9])?") {
            if label.len() >= MIN_NAME_LENGTH && label.len() <= MAX_NAME_LENGTH {
                let fqdn = format!("{}.xlm", label);
                prop_assert_eq!(parse_fqdn(&fqdn), Ok((label.clone(), Tld::Xlm)));
            }
        }
    }
}
