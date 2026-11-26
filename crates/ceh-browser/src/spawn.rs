//! Starts the browser process. On macOS the program that starts a process stays "responsible" for it, and
//! the system attributes the browser's own accesses (to its own `.app`) to that program: SantaCita then
//! showed "was prevented from modifying apps on your Mac". The browser is therefore started with the
//! responsibility disclaimed, so it answers for itself. Other systems start it as usual.
use std::{
    io::{self, Read},
    process::{Command, ExitStatus},
};

/// Where the browser's standard error goes; its standard input and output are always discarded.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Stderr {
    /// Every browser start now reads its output (for the report of a failed start); the tests of the process
    /// handling, which run on macOS only, still start one without.
    #[cfg_attr(not(all(test, target_os = "macos")), allow(dead_code))]
    Null,
    Piped,
}

/// A started browser process with the few operations the owner needs.
pub struct BrowserChild(Inner);

enum Inner {
    #[cfg(not(target_os = "macos"))]
    Std(std::process::Child),
    #[cfg(target_os = "macos")]
    Spawned(mac::Spawned),
}

pub fn spawn(command: Command, stderr: Stderr) -> io::Result<BrowserChild> {
    #[cfg(target_os = "macos")]
    {
        mac::spawn(&command, stderr).map(|spawned| BrowserChild(Inner::Spawned(spawned)))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let mut command = command;
        command.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null());
        command.stderr(match stderr {
            Stderr::Null => std::process::Stdio::null(),
            Stderr::Piped => std::process::Stdio::piped(),
        });
        command.spawn().map(|child| BrowserChild(Inner::Std(child)))
    }
}

impl BrowserChild {
    pub fn id(&self) -> u32 {
        match &self.0 {
            #[cfg(not(target_os = "macos"))]
            Inner::Std(child) => child.id(),
            #[cfg(target_os = "macos")]
            Inner::Spawned(child) => child.pid as u32,
        }
    }
    pub fn take_stderr(&mut self) -> Option<Box<dyn Read + Send>> {
        match &mut self.0 {
            #[cfg(not(target_os = "macos"))]
            Inner::Std(child) => child.stderr.take().map(|s| Box::new(s) as Box<dyn Read + Send>),
            #[cfg(target_os = "macos")]
            Inner::Spawned(child) => child.stderr.take().map(|s| Box::new(s) as Box<dyn Read + Send>),
        }
    }
    pub fn kill(&mut self) -> io::Result<()> {
        match &mut self.0 {
            #[cfg(not(target_os = "macos"))]
            Inner::Std(child) => child.kill(),
            #[cfg(target_os = "macos")]
            Inner::Spawned(child) => child.kill(),
        }
    }
    pub fn wait(&mut self) -> io::Result<ExitStatus> {
        match &mut self.0 {
            #[cfg(not(target_os = "macos"))]
            Inner::Std(child) => child.wait(),
            #[cfg(target_os = "macos")]
            Inner::Spawned(child) => child.wait(),
        }
    }
    pub fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        match &mut self.0 {
            #[cfg(not(target_os = "macos"))]
            Inner::Std(child) => child.try_wait(),
            #[cfg(target_os = "macos")]
            Inner::Spawned(child) => child.try_wait(),
        }
    }
}

#[cfg(target_os = "macos")]
mod mac {
    use super::Stderr;
    use std::{
        ffi::{CString, OsStr, OsString},
        fs::File,
        io,
        os::{
            fd::FromRawFd,
            unix::{ffi::OsStrExt, process::ExitStatusExt},
        },
        process::{Command, ExitStatus},
        ptr,
    };

    pub struct Spawned {
        pub pid: libc::pid_t,
        pub stderr: Option<File>,
        status: Option<ExitStatus>,
    }

    fn c_string(value: &OsStr) -> io::Result<CString> {
        CString::new(value.as_bytes()).map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))
    }

    fn check(code: libc::c_int) -> io::Result<()> {
        if code == 0 { Ok(()) } else { Err(io::Error::from_raw_os_error(code)) }
    }

    /// The environment the browser starts with: ours, with the command's changes applied.
    fn environment(command: &Command) -> io::Result<Vec<CString>> {
        let mut vars: Vec<(OsString, OsString)> = std::env::vars_os().collect();
        for (name, value) in command.get_envs() {
            vars.retain(|(existing, _)| existing != name);
            if let Some(value) = value {
                vars.push((name.to_owned(), value.to_owned()));
            }
        }
        vars.into_iter()
            .map(|(name, value)| {
                let mut entry = name.as_bytes().to_vec();
                entry.push(b'=');
                entry.extend_from_slice(value.as_bytes());
                CString::new(entry).map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))
            })
            .collect()
    }

    /// `responsibility_spawnattrs_setdisclaim` is in the system library (Chromium uses it the same way);
    /// when this system lacks it the browser is started without disclaiming, as before.
    fn disclaim(attributes: *mut libc::posix_spawnattr_t) {
        // SAFETY: the symbol, when present, has this C signature.
        unsafe {
            let symbol = libc::dlsym(libc::RTLD_DEFAULT, c"responsibility_spawnattrs_setdisclaim".as_ptr());
            if !symbol.is_null() {
                let function: unsafe extern "C" fn(*mut libc::posix_spawnattr_t, libc::c_int) -> libc::c_int =
                    std::mem::transmute(symbol);
                function(attributes, 1);
            }
        }
    }

    pub fn spawn(command: &Command, stderr: Stderr) -> io::Result<Spawned> {
        let program = c_string(command.get_program())?;
        let mut arguments = vec![program.clone()];
        for argument in command.get_args() {
            arguments.push(c_string(argument)?);
        }
        let mut argv: Vec<*mut libc::c_char> = arguments.iter().map(|a| a.as_ptr() as *mut libc::c_char).collect();
        argv.push(ptr::null_mut());
        let variables = environment(command)?;
        let mut envp: Vec<*mut libc::c_char> = variables.iter().map(|v| v.as_ptr() as *mut libc::c_char).collect();
        envp.push(ptr::null_mut());

        let mut pipe_ends = [-1 as libc::c_int; 2];
        if stderr == Stderr::Piped {
            // SAFETY: `pipe_ends` has room for the two descriptors.
            if unsafe { libc::pipe(pipe_ends.as_mut_ptr()) } != 0 {
                return Err(io::Error::last_os_error());
            }
            for end in pipe_ends {
                // SAFETY: both descriptors were just opened.
                unsafe { libc::fcntl(end, libc::F_SETFD, libc::FD_CLOEXEC) };
            }
        }
        let close_pipe = |ends: &[libc::c_int; 2]| {
            for end in ends {
                if *end >= 0 {
                    // SAFETY: the descriptor is open and owned here.
                    unsafe { libc::close(*end) };
                }
            }
        };

        // SAFETY: the structures are initialised before use and destroyed after; every pointer handed to the
        // system outlives the `posix_spawn` call.
        let result = unsafe {
            let mut actions: libc::posix_spawn_file_actions_t = std::mem::zeroed();
            let mut attributes: libc::posix_spawnattr_t = std::mem::zeroed();
            check(libc::posix_spawn_file_actions_init(&mut actions))?;
            check(libc::posix_spawnattr_init(&mut attributes))?;
            let outcome = (|| -> io::Result<libc::pid_t> {
                let null = c"/dev/null".as_ptr();
                check(libc::posix_spawn_file_actions_addopen(&mut actions, 0, null, libc::O_RDONLY, 0))?;
                check(libc::posix_spawn_file_actions_addopen(&mut actions, 1, null, libc::O_WRONLY, 0))?;
                if stderr == Stderr::Piped {
                    check(libc::posix_spawn_file_actions_adddup2(&mut actions, pipe_ends[1], 2))?;
                } else {
                    check(libc::posix_spawn_file_actions_addopen(&mut actions, 2, null, libc::O_WRONLY, 0))?;
                }
                // A browser that writes to a closed pipe must see the default SIGPIPE behaviour, as with `Command`.
                let mut default_signals: libc::sigset_t = std::mem::zeroed();
                libc::sigemptyset(&mut default_signals);
                libc::sigaddset(&mut default_signals, libc::SIGPIPE);
                check(libc::posix_spawnattr_setsigdefault(&mut attributes, &default_signals))?;
                let mut blocked: libc::sigset_t = std::mem::zeroed();
                libc::sigemptyset(&mut blocked);
                check(libc::posix_spawnattr_setsigmask(&mut attributes, &blocked))?;
                check(libc::posix_spawnattr_setflags(
                    &mut attributes,
                    (libc::POSIX_SPAWN_SETSIGDEF | libc::POSIX_SPAWN_SETSIGMASK) as libc::c_short,
                ))?;
                disclaim(&mut attributes);
                let mut pid: libc::pid_t = 0;
                check(libc::posix_spawn(&mut pid, program.as_ptr(), &actions, &attributes, argv.as_ptr(), envp.as_ptr()))?;
                Ok(pid)
            })();
            libc::posix_spawn_file_actions_destroy(&mut actions);
            libc::posix_spawnattr_destroy(&mut attributes);
            outcome
        };
        match result {
            Ok(pid) => {
                let stderr = if stderr == Stderr::Piped {
                    // SAFETY: the write end belongs to the browser now, the read end is ours alone.
                    unsafe { libc::close(pipe_ends[1]) };
                    Some(unsafe { File::from_raw_fd(pipe_ends[0]) })
                } else {
                    None
                };
                Ok(Spawned { pid, stderr, status: None })
            }
            Err(error) => {
                close_pipe(&pipe_ends);
                Err(error)
            }
        }
    }

    impl Spawned {
        pub fn kill(&mut self) -> io::Result<()> {
            if self.status.is_some() {
                return Ok(());
            }
            // SAFETY: plain signal to a child that has not been reaped.
            if unsafe { libc::kill(self.pid, libc::SIGKILL) } == 0 { Ok(()) } else { Err(io::Error::last_os_error()) }
        }
        pub fn wait(&mut self) -> io::Result<ExitStatus> {
            if let Some(status) = self.status {
                return Ok(status);
            }
            loop {
                let mut raw: libc::c_int = 0;
                // SAFETY: `raw` is a valid out pointer.
                let reaped = unsafe { libc::waitpid(self.pid, &mut raw, 0) };
                if reaped == self.pid {
                    let status = ExitStatus::from_raw(raw);
                    self.status = Some(status);
                    return Ok(status);
                }
                let error = io::Error::last_os_error();
                if error.kind() != io::ErrorKind::Interrupted {
                    return Err(error);
                }
            }
        }
        pub fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
            if let Some(status) = self.status {
                return Ok(Some(status));
            }
            let mut raw: libc::c_int = 0;
            // SAFETY: `raw` is a valid out pointer.
            let reaped = unsafe { libc::waitpid(self.pid, &mut raw, libc::WNOHANG) };
            match reaped {
                0 => Ok(None),
                pid if pid == self.pid => {
                    let status = ExitStatus::from_raw(raw);
                    self.status = Some(status);
                    Ok(Some(status))
                }
                _ => Err(io::Error::last_os_error()),
            }
        }
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    use std::process::Stdio;

    /// The process the system holds responsible for `pid` (`libsystem`, the call `launchd` tools use).
    fn responsible_for(pid: u32) -> Option<i32> {
        // SAFETY: the symbol has this signature in libsystem.
        unsafe {
            let symbol = libc::dlsym(libc::RTLD_DEFAULT, c"responsibility_get_pid_responsible_for_pid".as_ptr());
            if symbol.is_null() {
                return None;
            }
            let function: unsafe extern "C" fn(i32) -> i32 = std::mem::transmute(symbol);
            Some(function(pid as i32))
        }
    }

    #[test]
    fn an_ordinary_child_stays_the_responsibility_of_its_starter() {
        // The test runner can hand the responsibility for this process to another one while the test runs; the
        // starter is read before and after the child, and the child may answer to either.
        let before = responsible_for(std::process::id());
        let mut child = Command::new("/bin/sleep").arg("5").stdin(Stdio::null()).stdout(Stdio::null()).spawn().unwrap();
        let responsible = responsible_for(child.id());
        let after = responsible_for(std::process::id());
        let _ = child.kill();
        let _ = child.wait();
        // The baseline the fix is measured against: the child answers to whoever answers for its starter.
        assert!(responsible == before || responsible == after, "{responsible:?} is neither {before:?} nor {after:?}");
        assert_ne!(responsible, None);
    }

    #[test]
    fn the_browser_answers_for_itself() {
        let mut command = Command::new("/bin/sleep");
        command.arg("5");
        let mut child = spawn(command, Stderr::Null).unwrap();
        let responsible = responsible_for(child.id());
        child.kill().unwrap();
        child.wait().unwrap();
        assert_eq!(responsible, Some(child.id() as i32));
        assert_ne!(responsible, responsible_for(std::process::id()));
    }

    #[test]
    fn the_exit_code_the_output_and_the_environment_survive() {
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "echo \"$CEH_SPAWN_TEST${HOME:+ home}\" >&2; exit 3"]).env("CEH_SPAWN_TEST", "value").env_remove("HOME");
        let mut child = spawn(command, Stderr::Piped).unwrap();
        let mut text = String::new();
        child.take_stderr().unwrap().read_to_string(&mut text).unwrap();
        let status = child.wait().unwrap();
        assert_eq!(text.lines().last(), Some("value"));
        assert_eq!(status.code(), Some(3));
        assert!(child.try_wait().unwrap().is_some(), "a reaped child keeps its status");
    }

    #[test]
    fn a_running_child_is_reported_and_then_killed() {
        let mut command = Command::new("/bin/sleep");
        command.arg("30");
        let mut child = spawn(command, Stderr::Null).unwrap();
        assert!(child.try_wait().unwrap().is_none());
        child.kill().unwrap();
        assert!(!child.wait().unwrap().success());
    }

    #[test]
    fn a_program_that_does_not_exist_is_an_error() {
        assert!(spawn(Command::new("/nonexistent/browser"), Stderr::Piped).is_err());
    }
}
