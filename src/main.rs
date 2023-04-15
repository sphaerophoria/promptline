use std::{
    env,
    error::Error,
    fmt::{self, Write as FmtWrite},
    fs::{self, File},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

#[allow(unused)]
enum Color {
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
}

impl Color {
    fn to_ansi(&self) -> i32 {
        match self {
            Color::Red => 31,
            Color::Green => 32,
            Color::Yellow => 33,
            Color::Blue => 34,
            Color::Magenta => 35,
            Color::Cyan => 36,
            Color::White => 37,
        }
    }
}

enum DecoratedString {
    Bold(Box<DecoratedString>),
    Colored(Box<DecoratedString>, Color),
    Default(String),
}

impl DecoratedString {
    fn append_to_ansi(val: &DecoratedString, s: &mut String) -> Result<(), fmt::Error> {
        // https://gist.github.com/fnky/458719343aabd01cfb17a3a4f7296797
        match val {
            DecoratedString::Bold(inner) => {
                write!(s, "\x1b[1m")?;
                Self::append_to_ansi(inner, s)?;
                write!(s, "\x1b[22m")?;
            }
            DecoratedString::Colored(inner, color) => {
                write!(s, "\x1b[{}m", color.to_ansi())?;
                Self::append_to_ansi(inner, s)?;
                write!(s, "\x1b[39m")?;
            }
            DecoratedString::Default(val) => {
                write!(s, "{val}")?;
            }
        }

        Ok(())
    }

    fn to_ansi(&self) -> String {
        let mut ret = String::new();
        Self::append_to_ansi(self, &mut ret).unwrap();
        ret
    }

    fn bold(self) -> DecoratedString {
        DecoratedString::Bold(Box::new(self))
    }

    fn colored(self, color: Color) -> DecoratedString {
        DecoratedString::Colored(Box::new(self), color)
    }

    fn new(s: String) -> DecoratedString {
        DecoratedString::Default(s)
    }
}

fn get_time() -> String {
    let time = chrono::Local::now().time();
    let formatted = format!("{}", time.format("%H:%M"));
    DecoratedString::new(formatted)
        .bold()
        .colored(Color::Cyan)
        .to_ansi()
}

#[derive(Debug)]
enum UserError {
    GetUser(nix::Error),
    NoUser,
}

impl fmt::Display for UserError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            UserError::GetUser(_) => write!(f, "failed to get user"),
            UserError::NoUser => write!(f, "no active user"),
        }
    }
}

impl Error for UserError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            UserError::GetUser(e) => Some(e),
            UserError::NoUser => None,
        }
    }
}

fn get_user() -> Result<String, UserError> {
    let user = nix::unistd::User::from_uid(nix::unistd::getuid())
        .map_err(UserError::GetUser)?
        .ok_or(UserError::NoUser)?;

    let color_user = match user.name.as_str() {
        "root" => DecoratedString::new(user.name).colored(Color::Red).bold(),
        _ => DecoratedString::new(user.name)
            .colored(Color::Magenta)
            .bold(),
    };

    Ok(color_user.to_ansi())
}

#[derive(Debug)]
enum HostnameError {
    GetHostname(nix::Error),
    GetHostnameString(std::str::Utf8Error),
}

impl fmt::Display for HostnameError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            HostnameError::GetHostname(_) => write!(f, "failed to get host name"),
            HostnameError::GetHostnameString(_) => {
                write!(f, "failed to convert hostname to string")
            }
        }
    }
}

impl Error for HostnameError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            HostnameError::GetHostname(e) => Some(e),
            HostnameError::GetHostnameString(e) => Some(e),
        }
    }
}

fn get_hostname() -> Result<String, HostnameError> {
    let mut buf = [0u8; 64];
    let res = nix::unistd::gethostname(&mut buf)
        .map_err(HostnameError::GetHostname)?
        .to_str()
        .map_err(HostnameError::GetHostnameString)?;

    let res = DecoratedString::new(res.to_string())
        .colored(Color::Green)
        .bold()
        .to_ansi();

    Ok(res)
}

#[derive(Debug)]
struct NoExitStatus;

impl fmt::Display for NoExitStatus {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "no exit status")
    }
}

impl Error for NoExitStatus {}

fn get_status() -> Result<String, NoExitStatus> {
    let status = env::args().nth(1).ok_or(NoExitStatus)?;

    let color_status = match status.as_str() {
        "0" => DecoratedString::new(status)
            .colored(Color::Green)
            .bold()
            .to_ansi(),
        _ => DecoratedString::new(status)
            .colored(Color::Red)
            .bold()
            .to_ansi(),
    };

    Ok(color_status)
}

fn get_cwd() -> String {
    let cwd = env::var("PWD");

    if cwd.is_err() {
        return DecoratedString::new("!!!".to_string())
            .colored(Color::Red)
            .bold()
            .to_ansi();
    }

    let mut cwd = cwd.unwrap();

    if let Ok(home) = env::var("HOME") {
        if cwd.starts_with(&home) {
            cwd = cwd.replacen(&home, "~", 1);
        }
    }

    DecoratedString::new(cwd)
        .colored(Color::Blue)
        .bold()
        .to_ansi()
}

#[derive(Debug)]
enum HgError {
    NoCwd(std::io::Error),
    NotHg,
}

impl fmt::Display for HgError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            HgError::NoCwd(_) => write!(f, "failed to get working directory"),
            HgError::NotHg => write!(f, "working directory not in hg repo"),
        }
    }
}

impl Error for HgError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            HgError::NoCwd(e) => Some(e),
            HgError::NotHg => None,
        }
    }
}

fn get_mercurial_info() -> Result<String, HgError> {
    let mut hg_root = env::current_dir().map_err(HgError::NoCwd)?;

    loop {
        if hg_root.join(".hg").exists() {
            break;
        }

        if !hg_root.pop() {
            return Err(HgError::NotHg);
        }
    }

    let mut hg_components = vec![];

    {
        let mut maybe_push_file = |path| {
            // Don't care if this fails, just don't include it in the results
            let _ = File::open(hg_root.join(path))
                .and_then(|mut f| {
                    let mut output = String::new();
                    f.read_to_string(&mut output)?;
                    Ok(output)
                })
                .map(|s| hg_components.push(s.trim().to_string()));
        };

        maybe_push_file(hg_root.join(".hg/bookmarks.current"));
        maybe_push_file(hg_root.join(".hg/branch"));
    }

    let s = File::open(hg_root.join(".hg/dirstate"))
        .iter()
        .flat_map(|f| f.bytes())
        .take(6)
        .filter_map(|x| x.ok())
        .map(|x| format!("{x:02x}"))
        .collect::<String>();

    hg_components.push(s);

    let mut output = String::new();

    for (i, component) in hg_components.iter().enumerate() {
        if component.is_empty() {
            continue;
        }
        if i != 0 {
            output.push(' ')
        }
        output.push_str(component);
    }

    if output.is_empty() {
        return Ok(output.as_str().into());
    }

    let output = DecoratedString::new(output)
        .colored(Color::Green)
        .bold()
        .to_ansi();
    Ok(output)
}

#[derive(Debug)]
enum GitError {
    NoCwd(std::io::Error),
    CanonicalCwd(std::io::Error),
    ReadGitFile(std::io::Error),
    ReadHead(std::io::Error),
    NotGitRepo,
    UnexpectedGitContent,
    ReadRef(std::io::Error),
    NoRefName,
}

impl fmt::Display for GitError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            GitError::NoCwd(_) => write!(f, "failed to get cwd"),
            GitError::CanonicalCwd(_) => write!(f, "failed to canonicalize cwd"),
            GitError::ReadGitFile(_) => write!(f, "failed to read .git file"),
            GitError::ReadHead(_) => write!(f, "failed to read git HEAD"),
            GitError::NotGitRepo => write!(f, "not a git repo"),
            GitError::UnexpectedGitContent => write!(f, "unexpected git content"),
            GitError::ReadRef(_) => write!(f, "failed to read ref"),
            GitError::NoRefName => write!(f, "failed to get ref name"),
        }
    }
}

impl Error for GitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            GitError::NoCwd(e) => Some(e),
            GitError::CanonicalCwd(e) => Some(e),
            GitError::ReadGitFile(e) => Some(e),
            GitError::ReadHead(e) => Some(e),
            GitError::NotGitRepo => None,
            GitError::UnexpectedGitContent => None,
            GitError::ReadRef(e) => Some(e),
            GitError::NoRefName => None,
        }
    }
}

fn get_git_info() -> Result<String, GitError> {
    let cwd = env::current_dir().map_err(GitError::NoCwd)?;
    let canonical_cwd = fs::canonicalize(cwd).map_err(GitError::CanonicalCwd)?;

    let mut dir_iter = Some(&canonical_cwd as &Path);
    while let Some(dir) = dir_iter {
        if dir.join(".git").exists() {
            break;
        }

        dir_iter = dir.parent();
    }

    let repo = dir_iter.ok_or(GitError::NotGitRepo)?;

    // if .git has gitdir:.... we have to follow the link

    let mut git_dir = repo.join(".git");
    if git_dir.is_file() {
        let git_content = fs::read_to_string(git_dir).map_err(GitError::ReadGitFile)?;

        const PREFIX: &str = "gitdir: ";

        match git_content.strip_prefix(PREFIX) {
            Some(v) => git_dir = v.trim().into(),
            None => return Err(GitError::UnexpectedGitContent),
        }
    }

    let head_content = fs::read_to_string(git_dir.join("HEAD")).map_err(GitError::ReadHead)?;

    const REF_PREFIX: &str = "ref: ";
    let output = match head_content.strip_prefix(REF_PREFIX) {
        Some(refs_path) => {
            let refs_path = Path::new(refs_path.trim());

            let commit_hash =
                fs::read_to_string(git_dir.join(refs_path)).map_err(GitError::ReadRef)?;

            let short_hash = &commit_hash[..14];
            let ref_name = refs_path
                .file_name()
                .ok_or(GitError::NoRefName)?
                .to_string_lossy();

            format!("{ref_name} {short_hash}")
        }
        None => head_content[..14].to_string(),
    };

    Ok(DecoratedString::new(output)
        .colored(Color::Green)
        .bold()
        .to_ansi())
}

#[derive(Debug)]
struct NoCondaEnv;

impl fmt::Display for NoCondaEnv {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "no conda env var")
    }
}

impl Error for NoCondaEnv {}

fn get_conda_info() -> Result<String, NoCondaEnv> {
    let conda_env = std::env::var("CONDA_DEFAULT_ENV").map_err(|_| NoCondaEnv)?;
    Ok(DecoratedString::new(format!("🐍 {conda_env}"))
        .bold()
        .to_ansi())
}

#[derive(Debug)]
struct NotDockerContainer;

impl fmt::Display for NotDockerContainer {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "not docker container")
    }
}

impl Error for NotDockerContainer {}

fn get_docker_env() -> Result<String, NotDockerContainer> {
    match std::fs::metadata("/.dockerenv") {
        Ok(_) => Ok("🐳".into()),
        Err(_) => Err(NotDockerContainer),
    }
}

#[derive(Debug)]
enum ShellError {
    EnvNotSet,
    NoShellName,
}

impl fmt::Display for ShellError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ShellError::EnvNotSet => write!(f, "shell env not set"),
            ShellError::NoShellName => write!(f, "failed to get shell name"),
        }
    }
}

impl Error for ShellError {}

fn get_shell() -> Result<String, ShellError> {
    let shell: PathBuf = std::env::var("SHELL")
        .map_err(|_| ShellError::EnvNotSet)?
        .into();

    let name = shell
        .file_name()
        .ok_or(ShellError::NoShellName)?
        .to_string_lossy();

    Ok(DecoratedString::new(name.to_string()).bold().to_ansi())
}

#[derive(Debug)]
struct NotInNixShell;

impl fmt::Display for NotInNixShell {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "not in nix shell")
    }
}

impl Error for NotInNixShell {}

fn show_nix_shell() -> Result<String, NotInNixShell> {
    std::env::var("IN_NIX_SHELL").map_err(|_| NotInNixShell)?;

    let shell_name = std::env::var("name").unwrap_or("nix-shell".to_string());

    Ok(DecoratedString::new(format!("nix: {shell_name}"))
        .bold()
        .to_ansi())
}

fn do_print(mut components: Vec<String>) {
    components.insert(0, "┌[".into());
    for i in 1..components.len() - 1 {
        components.insert(2 * i, "]-[".into());
    }
    components.push("]\n└> ".into());
    for component in components {
        print!("{component}");
    }
}

#[derive(Debug)]
enum MainError {
    Docker(NotDockerContainer),
    User(UserError),
    Hostname(HostnameError),
    Shell(ShellError),
    Status(NoExitStatus),
    Mercurial(HgError),
    Git(GitError),
    Conda(NoCondaEnv),
    NixShell(NotInNixShell),
}

impl fmt::Display for MainError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let source: &dyn Error = match self {
            MainError::Docker(e) => {
                writeln!(f, "failed to get docker info")?;
                e
            }
            MainError::User(e) => {
                writeln!(f, "failed to get user info")?;
                e
            }
            MainError::Hostname(e) => {
                writeln!(f, "failed to get hostname info")?;
                e
            }
            MainError::Shell(e) => {
                writeln!(f, "failed to get shell info")?;
                e
            }
            MainError::Status(e) => {
                writeln!(f, "failed to get exit status")?;
                e
            }
            MainError::Mercurial(e) => {
                writeln!(f, "failed to get mercurial info")?;
                e
            }
            MainError::Git(e) => {
                writeln!(f, "failed to get git info")?;
                e
            }
            MainError::Conda(e) => {
                writeln!(f, "failed to get conda info")?;
                e
            }
            MainError::NixShell(e) => {
                writeln!(f, "failed to get nix shell info")?;
                e
            }
        };

        writeln!(f, "Caused by:")?;

        let mut source = Some(source);
        while let Some(err) = source {
            writeln!(f, "{err}")?;
            source = err.source();
        }

        Ok(())
    }
}

fn main() {
    let (oks, errors): (Vec<Result<_, MainError>>, Vec<_>) = vec![
        Ok(get_time()),
        get_docker_env().map_err(MainError::Docker),
        get_user().map_err(MainError::User),
        get_hostname().map_err(MainError::Hostname),
        Ok(get_cwd()),
        get_shell().map_err(MainError::Shell),
        get_status().map_err(MainError::Status),
        get_mercurial_info().map_err(MainError::Mercurial),
        get_git_info().map_err(MainError::Git),
        get_conda_info().map_err(MainError::Conda),
        show_nix_shell().map_err(MainError::NixShell),
    ]
    .into_iter()
    .partition(Result::is_ok);

    let components: Vec<_> = oks
        .into_iter()
        .map(|x| x.expect("Invalid result"))
        .collect();

    if Ok("1") == env::var("DEBUG_PROMPTLINE").as_ref().map(|s| s.as_str()) {
        for error in errors.into_iter().map(|e| e.unwrap_err()) {
            let _ = writeln!(io::stderr(), "{error}");
        }
    }
    do_print(components);
}
