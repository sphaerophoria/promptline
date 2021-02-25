#[macro_use] extern crate error_chain;
extern crate ansi_term;
extern crate chrono;
extern crate hostname;
extern crate user;
extern crate git2;

use ansi_term::Colour::*;
use ansi_term::{ANSIGenericString, ANSIGenericStrings};
use std::io::{self, Read, Write};
use std::fs::File;
use std::env;

mod errors {
    error_chain! { }
}

use errors::*;

fn get_time<'a>() -> Result<ANSIGenericString<'a, str>> {
    let time = chrono::Local::now().time();
    Ok(Cyan.bold().paint(format!("{}", time.format("%H:%M"))))
}

fn get_user<'a>() -> Result<ANSIGenericString<'a, str>> {
    let user = user::get_user_name()
        .chain_err(|| "Failed to retrieve username")?;

    let color_user = match user.as_str() {
        "root" => Red.bold().paint(user),
        _ => Purple.bold().paint(user),
    };

    Ok(color_user)
}

fn get_hostname<'a>() -> Result<ANSIGenericString<'a, str>> {
    let hostname = hostname::get_hostname()
        .ok_or("Failed to get hostname")?;

    let color_hostname = Green.bold().paint(hostname);

    Ok(color_hostname)
}

fn get_status<'a>() -> Result<ANSIGenericString<'a, str>> {
    let status = env::args().nth(1).ok_or("No exit status")?;

    let color_status  = match status.as_str() {
        "0" => Green.bold().paint(status),
        _ => Red.bold().paint(status),
    };

    Ok(color_status)
}

fn get_cwd<'a>() -> Result<ANSIGenericString<'a, str>> {

    let cwd = env::var("PWD");

    if cwd.is_err() {
        return Ok(Red.bold().paint("!!!"));
    }

    let mut cwd = cwd.unwrap();

    if let Ok(home) = env::var("HOME") {
        if cwd.starts_with(&home) {
            cwd = cwd.replacen(&home, "~", 1);
        }
    }

    Ok(Blue.bold().paint(cwd))
}

fn get_mercurial_info<'a>() -> Result<ANSIGenericString<'a, str>> {
    let mut hg_root = env::current_dir().chain_err(|| "No cwd")?;

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
        .map(|x| format!("{:x}", x))
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
        return Ok(ANSIGenericString::from(output));
    }

    let output = Green.bold().paint(output);
    Ok(output)
}

fn get_git_info<'a>() ->  Result<ANSIGenericString<'a, str>>{
    use git2::Repository;
    let get_git_name  = || -> std::result::Result<String, git2::Error> {
        let repo = Repository::discover(".")?;
        let head = repo.head()?;
        let name = head.shorthand().ok_or(git2::Error::from_str("Failed to get name"))?;
        let oid = head.target().ok_or(git2::Error::from_str("Failed to get target"))?;
        let oid_str = oid.as_bytes().iter().take(6).map(|x| format!("{:x}", x)).collect::<String>();
        Ok(format!("{} {}", name, oid_str))
    };

    match get_git_name() {
      Ok(s) => Ok(Green.bold().paint(s)),
      Err(_) => bail!("Not in git folder"),
    }
}

fn do_print<'a>(mut components: Vec<ANSIGenericString<'a, str>>) {
    components.insert(0,ANSIGenericString::from("┌["));
    for i in 1..components.len() - 1 {
        components.insert(2*i, ANSIGenericString::from("]-["));
    }
    components.push(ANSIGenericString::from("]\n└> "));
    print!("{}", ANSIGenericStrings(&components));
}

quick_main!(run);
fn run() -> Result<()> {
    let (oks, errors): (Vec<Result<_>>, _) = vec![
        get_time(),
        get_user(),
        get_hostname(),
        get_cwd(),
        get_status(),
        get_mercurial_info(),
        get_git_info(),
    ]
    .into_iter()
    .partition(|x| x.is_ok());

    let components: Vec<_> = oks.into_iter().map(|x| x.unwrap()).collect();

    if Ok("1") == env::var("DEBUG_PROMPTLINE").as_ref().map(|s| s.as_str()) {
        for error in errors.into_iter().map(|e| e.unwrap_err()) {
            let _ = write!(io::stderr(), "{}\n", error);
        }
    }
    do_print(components);
    Ok(())
}
