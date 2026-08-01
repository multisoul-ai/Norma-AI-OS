use std::fmt;

const GITHUB_HOST: &str = "github.com";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CloneTransport {
    Https,
    Ssh,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GitHubRemote {
    owner: String,
    repository: String,
    transport: CloneTransport,
}

impl GitHubRemote {
    pub fn parse(input: &str) -> Result<Self, GitHubRemoteParseError> {
        if input.chars().any(char::is_control) {
            return Err(GitHubRemoteParseError::InvalidFormat);
        }
        let input = input.trim().trim_end_matches('/');
        if input.is_empty() {
            return Err(GitHubRemoteParseError::Empty);
        }

        let (owner, repository, transport) = if let Some(rest) = input.strip_prefix("https://") {
            parse_https(rest)?
        } else if let Some(rest) = input.strip_prefix("ssh://") {
            parse_ssh_url(rest)?
        } else if let Some(rest) = input.strip_prefix("git@") {
            parse_scp_ssh(rest)?
        } else if input.contains("://") {
            return Err(GitHubRemoteParseError::UnsupportedScheme);
        } else {
            return Err(GitHubRemoteParseError::InvalidFormat);
        };

        validate_owner(owner)?;
        let repository = repository.strip_suffix(".git").unwrap_or(repository);
        validate_repository(repository)?;

        Ok(Self {
            owner: owner.to_string(),
            repository: repository.to_string(),
            transport,
        })
    }

    pub fn owner(&self) -> &str {
        &self.owner
    }

    pub fn repository(&self) -> &str {
        &self.repository
    }

    pub fn transport(&self) -> CloneTransport {
        self.transport
    }

    pub fn url_for(&self, transport: CloneTransport) -> String {
        match transport {
            CloneTransport::Https => {
                format!(
                    "https://{GITHUB_HOST}/{}/{}.git",
                    self.owner, self.repository
                )
            }
            CloneTransport::Ssh => {
                format!("git@{GITHUB_HOST}:{}/{}.git", self.owner, self.repository)
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GitHubRemoteParseError {
    Empty,
    UnsupportedScheme,
    UnsupportedHost,
    EmbeddedCredentials,
    InvalidRepositoryPath,
    InvalidOwner,
    InvalidRepository,
    InvalidFormat,
}

impl fmt::Display for GitHubRemoteParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::Empty => "Enter a GitHub repository URL.",
            Self::UnsupportedScheme => "Use a GitHub HTTPS or SSH repository URL.",
            Self::UnsupportedHost => "Only github.com repositories are supported.",
            Self::EmbeddedCredentials => {
                "Do not include usernames, tokens, or passwords in the repository URL."
            }
            Self::InvalidRepositoryPath => {
                "Enter a repository URL in the form github.com/owner/repository."
            }
            Self::InvalidOwner => "The GitHub owner name is invalid.",
            Self::InvalidRepository => "The GitHub repository name is invalid.",
            Self::InvalidFormat => "Enter a valid GitHub HTTPS or SSH repository URL.",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for GitHubRemoteParseError {}

fn parse_https(rest: &str) -> Result<(&str, &str, CloneTransport), GitHubRemoteParseError> {
    let (authority, path) = rest
        .split_once('/')
        .ok_or(GitHubRemoteParseError::InvalidRepositoryPath)?;
    if authority.contains('@') {
        return Err(GitHubRemoteParseError::EmbeddedCredentials);
    }
    validate_host(authority)?;
    let (owner, repository) = parse_repository_path(path)?;
    Ok((owner, repository, CloneTransport::Https))
}

fn parse_ssh_url(rest: &str) -> Result<(&str, &str, CloneTransport), GitHubRemoteParseError> {
    let (authority, path) = rest
        .split_once('/')
        .ok_or(GitHubRemoteParseError::InvalidRepositoryPath)?;
    let (user, host) = authority
        .split_once('@')
        .ok_or(GitHubRemoteParseError::InvalidFormat)?;
    if user != "git" {
        return Err(GitHubRemoteParseError::InvalidFormat);
    }
    validate_host(host)?;
    let (owner, repository) = parse_repository_path(path)?;
    Ok((owner, repository, CloneTransport::Ssh))
}

fn parse_scp_ssh(rest: &str) -> Result<(&str, &str, CloneTransport), GitHubRemoteParseError> {
    let (host, path) = rest
        .split_once(':')
        .ok_or(GitHubRemoteParseError::InvalidFormat)?;
    validate_host(host)?;
    let (owner, repository) = parse_repository_path(path)?;
    Ok((owner, repository, CloneTransport::Ssh))
}

fn validate_host(host: &str) -> Result<(), GitHubRemoteParseError> {
    if host.eq_ignore_ascii_case(GITHUB_HOST) {
        Ok(())
    } else {
        Err(GitHubRemoteParseError::UnsupportedHost)
    }
}

fn parse_repository_path(path: &str) -> Result<(&str, &str), GitHubRemoteParseError> {
    if path.contains('?') || path.contains('#') {
        return Err(GitHubRemoteParseError::InvalidRepositoryPath);
    }
    let mut segments = path.split('/');
    let owner = segments
        .next()
        .filter(|segment| !segment.is_empty())
        .ok_or(GitHubRemoteParseError::InvalidRepositoryPath)?;
    let repository = segments
        .next()
        .filter(|segment| !segment.is_empty())
        .ok_or(GitHubRemoteParseError::InvalidRepositoryPath)?;
    if segments.next().is_some() {
        return Err(GitHubRemoteParseError::InvalidRepositoryPath);
    }
    Ok((owner, repository))
}

fn validate_owner(owner: &str) -> Result<(), GitHubRemoteParseError> {
    let valid_length = (1..=39).contains(&owner.len());
    let valid_characters = owner
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-');
    let bounded_by_alphanumeric = owner
        .as_bytes()
        .first()
        .is_some_and(u8::is_ascii_alphanumeric)
        && owner
            .as_bytes()
            .last()
            .is_some_and(u8::is_ascii_alphanumeric);
    if valid_length && valid_characters && bounded_by_alphanumeric {
        Ok(())
    } else {
        Err(GitHubRemoteParseError::InvalidOwner)
    }
}

fn validate_repository(repository: &str) -> Result<(), GitHubRemoteParseError> {
    let valid_length = (1..=100).contains(&repository.len());
    let valid_characters = repository
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'));
    let is_dot_path = matches!(repository, "." | "..");
    if valid_length && valid_characters && !is_dot_path {
        Ok(())
    } else {
        Err(GitHubRemoteParseError::InvalidRepository)
    }
}
