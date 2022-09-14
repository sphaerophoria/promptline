use anyhow::{anyhow, bail, Context, Result};
use colored::{Colorize, ColoredString};
use std::{env, fs::File, io::{self, Read, Write}, path::PathBuf};

fn get_time() -> Result<ColoredString> {
    let time = chrono::Local::now().time();
    Ok(format!("{}", time.format("%H:%M")).cyan().bold())
}

fn get_user() -> Result<ColoredString> {
    let user = nix::unistd::User::from_uid(nix::unistd::getuid())
        .into_iter()
        .flatten()
        .next()
        .ok_or(anyhow!("Failed to get user"))?;

    let color_user = match user.name.as_str() {
        "root" => user.name.red().bold(),
        _ => user.name.purple().bold(),
    };

    Ok(color_user)
}

fn get_hostname() -> Result<ColoredString> {
    let mut buf = [0u8; 64];
    let res = nix::unistd::gethostname(&mut buf)
        .context("Failed to retrieve hostname")?
        .to_str()
        .context("Hostname not a valid utf8 string")?
        .green()
        .bold();

    Ok(res)
}

fn get_status() -> Result<ColoredString> {
    let status = env::args().nth(1).ok_or(anyhow!("No exit status"))?;

    let color_status  = match status.as_str() {
        "0" => status.green().bold(),
        _ => status.red().bold()
    };

    Ok(color_status)
}

fn get_cwd() -> Result<ColoredString> {

    let cwd = env::var("PWD");

    if cwd.is_err() {
        return Ok("!!!".red().bold());
    }

    let mut cwd = cwd.unwrap();

    if let Ok(home) = env::var("HOME") {
        if cwd.starts_with(&home) {
            cwd = cwd.replacen(&home, "~", 1);
        }
    }

    Ok(cwd.blue().bold())
}

fn get_mercurial_info() -> Result<ColoredString> {
    let mut hg_root = env::current_dir().with_context(|| "No cwd")?;

    loop {
        if hg_root.join(".hg").exists() {
            break;
        }

        if !hg_root.pop() {
            bail!("Not in hg folder");
        }
    }

    let mut hg_components = vec![];

    {
        let mut maybe_push_file = |path| {
            // Don't care if this fails, just don't include it in the results
            let _ = File::open(hg_root.join(path))
                .and_then(|mut f|{
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
        .map(|x| format!("{:02x}", x))
        .collect::<String>();

    hg_components.push(s);

    let mut output = String::new();

    for (i, component) in hg_components.iter().enumerate() {
        if component.is_empty() { continue; }
        if i != 0 {
            output.push(' ')
        }
        output.push_str(&component);
    }

    if output.is_empty() {
        return Ok(output.as_str().into());
    }

    let output = output.green().bold();
    Ok(output)
}

fn get_git_info() ->  Result<ColoredString>{
    use git2::Repository;
    let get_git_name  = || -> std::result::Result<String, git2::Error> {
        let repo = Repository::discover(".")?;
        let head = repo.head()?;
        let name = head.shorthand().ok_or(git2::Error::from_str("Failed to get name"))?;
        let oid = head.target().ok_or(git2::Error::from_str("Failed to get target"))?;
        let oid_str = oid.as_bytes().iter().take(6).map(|x| format!("{:02x}", x)).collect::<String>();
        Ok(format!("{} {}", name, oid_str))
    };

    match get_git_name() {
      Ok(s) => Ok(s.green().bold()),
      Err(_) => bail!("Not in git folder"),
    }
}

fn get_conda_info() -> Result<ColoredString> {
    let conda_env = match std::env::var("CONDA_DEFAULT_ENV") {
        Ok(v) => v,
        Err(_) => bail!("No conda env")
    };

    Ok(format!("🐍 {}", conda_env).bold())
}

fn get_docker_env() -> Result<ColoredString> {
    match std::fs::metadata("/.dockerenv") {
        Ok(_) => Ok("🐳".into()),
        Err(_) => bail!("Not in docker container")
    }
}

fn get_shell() -> Result<ColoredString> {
    let shell: PathBuf = std::env::var("SHELL")
        .context("SHELL not set")?
        .into();

    let name = shell.file_name()
        .ok_or(anyhow!("Failed to get shell name"))?
        .to_str()
        .ok_or(anyhow!("Failed to convert shell name to string"))?;

    Ok(name.bold())
}

fn show_nix_shell() -> Result<ColoredString> {

    std::env::var("IN_NIX_SHELL")
        .context("Not in nix shell")?;

    let shell_name = std::env::var("name")
        .unwrap_or("nix-shell".to_string());

    Ok(format!("nix: {}", shell_name).bold())
}

fn do_print(mut components: Vec<ColoredString>) {
    components.insert(0,"┌[".into());
    for i in 1..components.len() - 1 {
        components.insert(2*i, "]-[".into());
    }
    components.push("]\n└> ".into());
    for component in components {
        print!("{}", component);
    }
}

fn main() -> Result<()> {
    colored::control::set_override(true);

    let (oks, errors): (Vec<Result<_>>, _) = vec![
        get_time(),
        get_docker_env(),
        get_user(),
        get_hostname(),
        get_cwd(),
        get_shell(),
        get_status(),
        get_mercurial_info(),
        get_git_info(),
        get_conda_info(),
        show_nix_shell(),
    ]
    .into_iter()
    .partition(|x| x.is_ok());

    let components: Vec<_> = oks.into_iter().map(|x| x.unwrap()).collect();

    if Ok("1") == env::var("DEBUG_PROMPTLINE").as_ref().map(|s| s.as_str()) {
        for error in errors.into_iter().map(|e| e.unwrap_err()) {
            let _ = write!(io::stderr(), "{:?}\n", error);
        }
    }
    do_print(components);
    Ok(())
}
