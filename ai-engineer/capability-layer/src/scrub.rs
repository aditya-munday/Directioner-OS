use std::io::Read;

pub const MAX_OUTPUT_BYTES: usize = 64 * 1024;
pub const TRUNCATION_MARKER: &str = "\n[OUTPUT TRUNCATED: exceeded 64 KiB limit]\n";

pub struct SecretPattern {
    pub prefix: &'static str,
    pub redaction: &'static str,
}

pub const SECRET_PREFIXES: &[SecretPattern] = &[
    SecretPattern {
        prefix: "-----BEGIN RSA PRIVATE KEY-----",
        redaction: "[REDACTED RSA PRIVATE KEY]",
    },
    SecretPattern {
        prefix: "-----BEGIN OPENSSH PRIVATE KEY-----",
        redaction: "[REDACTED OPENSSH PRIVATE KEY]",
    },
    SecretPattern {
        prefix: "-----BEGIN PRIVATE KEY-----",
        redaction: "[REDACTED PRIVATE KEY]",
    },
    SecretPattern {
        prefix: "-----BEGIN EC PRIVATE KEY-----",
        redaction: "[REDACTED EC PRIVATE KEY]",
    },
    SecretPattern {
        prefix: "password=",
        redaction: "password=[REDACTED]",
    },
    SecretPattern {
        prefix: "PASSWORD=",
        redaction: "PASSWORD=[REDACTED]",
    },
    SecretPattern {
        prefix: "token=",
        redaction: "token=[REDACTED]",
    },
    SecretPattern {
        prefix: "TOKEN=",
        redaction: "TOKEN=[REDACTED]",
    },
    SecretPattern {
        prefix: "api_key=",
        redaction: "api_key=[REDACTED]",
    },
    SecretPattern {
        prefix: "API_KEY=",
        redaction: "API_KEY=[REDACTED]",
    },
    SecretPattern {
        prefix: "Authorization: Bearer ",
        redaction: "Authorization: Bearer [REDACTED]",
    },
];

pub fn read_bounded_output<R: Read>(mut reader: R, limit: usize) -> (Vec<u8>, bool) {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 8192];
    let mut truncated = false;

    while buf.len() < limit {
        let to_read = (limit - buf.len()).min(chunk.len());
        match reader.read(&mut chunk[..to_read]) {
            Ok(0) => break,
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
            Err(_) => break,
        }
    }

    let mut check_extra = [0u8; 1];
    if let Ok(n) = reader.read(&mut check_extra)
        && n > 0
    {
        truncated = true;
    }

    if truncated {
        buf.extend_from_slice(TRUNCATION_MARKER.as_bytes());
    }

    (buf, truncated)
}

pub fn scrub_secrets(input: &str) -> (String, bool) {
    let mut output = input.to_string();
    let mut scrubbed = false;

    for pat in SECRET_PREFIXES {
        if let Some(pos) = output.find(pat.prefix) {
            scrubbed = true;
            if pat.prefix.starts_with("-----BEGIN") {
                let end_marker = pat.prefix.replace("BEGIN", "END");
                if let Some(end_pos) = output[pos..].find(&end_marker) {
                    let total_end = pos + end_pos + end_marker.len();
                    output.replace_range(pos..total_end, pat.redaction);
                } else {
                    output.replace_range(pos.., pat.redaction);
                }
            } else if let Some(newline_or_space) = output[pos..].find(['\n', ' ', '&']) {
                let total_end = pos + newline_or_space;
                output.replace_range(pos..total_end, pat.redaction);
            } else {
                output.replace_range(pos.., pat.redaction);
            }
        }
    }

    (output, scrubbed)
}
