//! Welcome to the matrix-commander crate!
//!
//! Please help create the Rust version of matrix-commander.
//! Please consider providing Pull Requests.
//! Have a look at: <https://github.com/8go/matrix-commander-rs>
//!
//! `matrix-commander-rs` is a (partial initial) re-implementation
//! of the feature-rich `matrix-commander` (Python) program with
//! its repo at <https://github.com/8go/matrix-commander>.
//!
//! matrix-commander is a simple terminal-based CLI client of
//! Matrix <https://matrix.org>. It let's you login to your
//! Matrix account, verify your new devices, and send encrypted
//! (or not-encrypted) messages and files on the Matrix network.
//!
//! For building from source in Rust you require the
//! OpenSsl development library. Install it first, e.g. on
//! Fedora you would `sudo dnf install openssl-devel` or on
//! Ubuntu you would `sudo apt install libssl-dev`.
//!
//! Please help improve the code and add features  :pray:  :clap:
//!
//! Usage:
//! - matrix-commander-rs --login password # first time only
//! - matrix-commander-rs --verify # emoji verification
//! - matrix-commander-rs --message "Hello World"
//! - matrix-commander-rs --file test.txt
//! - or do many things at a time:
//! - matrix-commander-rs --message Hi --file test.txt --devices --logout me
//!
//! For more information, see the README.md
//! <https://github.com/8go/matrix-commander-rs/blob/main/README.md>
//! file.

// #![allow(dead_code)] // crate-level allow  // Todo
// #![allow(unused_variables)] // Todo
// #![allow(unused_imports)] // Todo

use atty::Stream;
use std::env;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::Path;
use std::path::PathBuf;
use tracing::{debug, enabled, /* warn, */ error, info, Level};
// Collect, List, Store, StoreFalse,
use argparse::{ArgumentParser, IncrBy, StoreOption, StoreTrue};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;

use matrix_sdk::{
    // config::{RequestConfig, StoreConfig, SyncSettings},
    // instant::Duration,
    // room,
    ruma::{
        OwnedDeviceId,
        OwnedUserId,
        // device_id, room_id, session_id, user_id, OwnedRoomId,  RoomId,
    },
    Client,
    Session,
};

mod mclient; // import matrix-sdk Client related code
use crate::mclient::{devices, file, login, logout, message, restore_login, verify};

/// the version number from Cargo.toml at compile time
const VERSION_O: Option<&str> = option_env!("CARGO_PKG_VERSION");
/// fallback if static compile time value is None
const VERSION: &str = "unknown version";
/// the package name from Cargo.toml at compile time, usually matrix-commander
const PKG_NAME_O: Option<&str> = option_env!("CARGO_PKG_NAME");
/// fallback if static compile time value is None
const PKG_NAME: &str = "matrix-commander";
/// he repo name from Cargo.toml at compile time,
/// e.g. string `https://github.com/8go/matrix-commander-rs/`
const PKG_REPOSITORY_O: Option<&str> = option_env!("CARGO_PKG_REPOSITORY");
/// fallback if static compile time value is None
const PKG_REPOSITORY: &str = "https://github.com/8go/matrix-commander-rs/";
/// default name for login credentials JSON file
const CREDENTIALS_FILE_DEFAULT: &str = "credentials.json";
/// default directory to be used by end-to-end encrypted protocol for persistent storage
const SLEDSTORE_DIR_DEFAULT: &str = "sledstore/";
/// default timeouts for waiting for the Matrix server, in seconds
const TIMEOUT_DEFAULT: u64 = 60;

/// The enumerator for Errors
#[derive(Error, Debug)]
pub enum Error {
    #[error("{0}")]
    Custom(&'static str),

    #[error("No valid home directory path")]
    NoNomeDirectory,

    #[error("Not logged in")]
    NotLoggedIn,

    #[error("Invalid Room")]
    InvalidRoom,

    #[error("Invalid File")]
    InvalidFile,

    #[error("Login Failed")]
    LoginFailed,

    #[error("Login Unnecessary")]
    LoginUnnecessary,

    #[error("Send Failed")]
    SendFailed,

    #[error("Restoring Login Failed")]
    RestoreLoginFailed,

    #[error("Invalid Client Connection")]
    InvalidClientConnection,

    #[error("Unknown CLI parameter")]
    UnknownCliParameter,

    #[error("Unsupported CLI parameter")]
    UnsupportedCliParameter,

    #[error("Missing CLI parameter")]
    MissingCliParameter,

    #[error("Not Implemented Yet")]
    NotImplementedYet,

    #[error(transparent)]
    IO(#[from] std::io::Error),

    #[error(transparent)]
    Matrix(#[from] matrix_sdk::Error),

    #[error(transparent)]
    Json(#[from] serde_json::Error),

    #[error(transparent)]
    Http(#[from] matrix_sdk::HttpError),
}

// impl Error {
//     pub(crate) fn custom<T>(message: &'static str) -> Result<T> {
//         Err(Error::Custom(message))
//     }
// }

/// Trivial definition of Result type
pub(crate) type Result<T = ()> = std::result::Result<T, Error>;

/// A public struct with private fields to keep the command line arguments from
/// library `argparse`.
#[derive(Clone, Debug)]
pub struct Args {
    contribute: bool,
    version: bool,
    debug: usize,
    log_level: Option<String>,
    verbose: usize,
    login: Option<String>,
    verify: bool,
    message: Option<String>,
    logout: Option<String>,
    homeserver: Option<Url>,
    user_login: Option<String>,
    password: Option<String>,
    device: Option<String>,
    room_default: Option<String>,
    devices: bool,
    timeout: Option<u64>,
    markdown: bool,
    code: bool,
    room: Option<String>,
    file: Option<String>,
    notice: bool,
    emote: bool,
}

impl Args {
    pub fn new() -> Args {
        Args {
            contribute: false,
            version: false,
            debug: 0usize,
            log_level: None,
            verbose: 0usize,
            login: None,
            verify: false,
            message: None,
            logout: None,
            homeserver: None,
            user_login: None,
            password: None,
            device: None,
            room_default: None,
            devices: false,
            timeout: Some(TIMEOUT_DEFAULT),
            markdown: false,
            code: false,
            room: None,
            file: None,
            notice: false,
            emote: false,
        }
    }
}

/// A struct for the credentials. These will be serialized into JSON
/// and written to the credentials.json file for permanent storage and
/// future access.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Credentials {
    homeserver: Url,
    user_id: OwnedUserId,
    access_token: String,
    device_id: OwnedDeviceId,
    room_default: String,
    refresh_token: Option<String>,
}

// credentials = Credentials::new(
//     Url::from_file_path("/a").expect("url bad"), // homeserver: Url,
//     user_id!(r"@a:a").to_owned(), // user_id: OwnedUserId,
//     String::new().to_owned(), // access_token: String,
//     device_id!("").to_owned(), // device_id: OwnedDeviceId,
//     String::new(), // room_default: String,
//     None, // refresh_token: Option<String>
// ),

impl AsRef<Credentials> for Credentials {
    fn as_ref(&self) -> &Self {
        self
    }
}

/// implementation of Credentials struct
impl Credentials {
    /// Constructor for Credentials
    fn load(path: &Path) -> Result<Credentials> {
        let reader = File::open(path)?;
        Credentials::set_permissions(&reader)?;
        let credentials: Credentials = serde_json::from_reader(reader)?;
        let mut credentialsfiltered = credentials.clone();
        credentialsfiltered.access_token = "***".to_string();
        info!("loaded credentials are: {:?}", credentialsfiltered);
        Ok(credentials)
    }

    /// Writing the credentials to a file
    fn save(&self, path: &Path) -> Result {
        fs::create_dir_all(path.parent().ok_or(Error::NoNomeDirectory)?)?;
        let writer = File::create(path)?;
        serde_json::to_writer_pretty(&writer, self)?;
        Credentials::set_permissions(&writer)?;
        Ok(())
    }

    #[cfg(unix)]
    fn set_permissions(file: &File) -> Result {
        use std::os::unix::fs::PermissionsExt;
        let perms = file.metadata()?.permissions();
        // is the file world-readable? if so, reset the permissions to 600
        if perms.mode() & 0o4 == 0o4 {
            file.set_permissions(fs::Permissions::from_mode(0o600))
                .unwrap();
        }
        Ok(())
    }

    #[cfg(not(unix))]
    fn set_permissions(file: &File) -> Result {
        Ok(())
    }

    /// Default constructor
    fn new(
        homeserver: Url,
        user_id: OwnedUserId,
        access_token: String,
        device_id: OwnedDeviceId,
        room_default: String,
        refresh_token: Option<String>,
    ) -> Self {
        Self {
            homeserver,
            user_id,
            access_token,
            device_id,
            room_default,
            refresh_token,
        }
    }
}

/// Implements From trait for Session
impl From<Credentials> for Session {
    fn from(creditials: Credentials) -> Self {
        Self {
            user_id: creditials.user_id,
            access_token: creditials.access_token,
            device_id: creditials.device_id,
            // no default_room in session
            refresh_token: creditials.refresh_token,
        }
    }

    //
    // From matrix-sdk doc
    // pub struct Session {
    //     pub access_token: String,
    //     pub refresh_token: Option<String>,
    //     pub user_id: OwnedUserId,
    //     pub device_id: OwnedDeviceId,
    // }
    //
    // A user session, containing an access token, an optional refresh token
    // and information about the associated user account.
    // Example
    //
    // use matrix_sdk_base::Session;
    // use ruma::{device_id, user_id};
    //
    // let session = Session {
    //     access_token: "My-Token".to_owned(),
    //     refresh_token: None,
    //     user_id: user_id!("@example:localhost").to_owned(),
    //     device_id: device_id!("MYDEVICEID").to_owned(),
    // };
    //
    // assert_eq!(session.device_id.as_str(), "MYDEVICEID");
}

/// A public struct with a private fields to keep the global state
#[derive(Clone)]
pub struct GlobalState {
    // self.log: logging.Logger = None  # logger object
    ap: Args, // parsed arguments
    // # to which logic (message, image, audio, file, event) is
    // # stdin pipe assigned?
    // self.stdin_use: str = "none"
    // # 1) ssl None means default SSL context will be used.
    // # 2) ssl False means SSL certificate validation will be skipped
    // # 3) ssl a valid SSLContext means that the specified context will be
    // #    used. This is useful to using local SSL certificate.
    // self.ssl: Union[None, SSLContext, bool] = None
    //client: AsyncClient,
    // client: Option<String>,
    credentials_file_path: PathBuf,
    sledstore_dir_path: PathBuf,
    // Session info and a bit more
    credentials: Option<Credentials>,
    // self.send_action = False  # argv contains send action
    // self.listen_action = False  # argv contains listen action
    // self.room_action = False  # argv contains room action
    // self.set_action = False  # argv contains set action
    // self.get_action = False  # argv contains get action
    // self.setget_action = False  # argv contains set or get action
    // self.err_count = 0  # how many errors have occurred so far
    // self.warn_count = 0  # how many warnings have occurred so far
}

/// Implementation of the GlobalState struct.
impl GlobalState {
    /// Default constructor of GlobalState
    pub fn new(_arg: String) -> GlobalState {
        GlobalState {
            ap: Args::new(),
            // e.g. /home/user/.local/share/matrix-commander/credentials.json
            credentials_file_path: get_credentials_default_path(),
            sledstore_dir_path: get_sledstore_default_path(),
            credentials: None, // Session info and a bit more
        }
    }
}

/// Gets the *default* path (including file name) of the credentials file
/// The default path might not be the actual path as it can be overwritten with command line
/// options.
fn get_credentials_default_path() -> PathBuf {
    let dir = ProjectDirs::from_path(PathBuf::from(get_prog_without_ext())).unwrap();
    // fs::create_dir_all(dir.data_dir());
    let dp = dir.data_dir().join(CREDENTIALS_FILE_DEFAULT);
    debug!(
        "Data will be put into project directory {:?}.",
        dir.data_dir()
    );
    info!("Credentials file with access token is {}.", dp.display());
    dp
}

/// Gets the *actual* path (including file name) of the credentials file
/// The default path might not be the actual path as it can be overwritten with command line
/// options.
#[allow(dead_code)]
fn get_credentials_actual_path(gs: &GlobalState) -> &PathBuf {
    &gs.credentials_file_path
}

/// Gets the *default* path (terminating in a directory) of the sled store directory
/// The default path might not be the actual path as it can be overwritten with command line
/// options.
fn get_sledstore_default_path() -> PathBuf {
    let dir = ProjectDirs::from_path(PathBuf::from(get_prog_without_ext())).unwrap();
    // fs::create_dir_all(dir.data_dir());
    let dp = dir.data_dir().join(SLEDSTORE_DIR_DEFAULT);
    debug!(
        "Data will be put into project directory {:?}.",
        dir.data_dir()
    );
    info!("Sled store directory is {}.", dp.display());
    dp
}

/// Gets the *actual* path (including file name) of the sled store directory
/// The default path might not be the actual path as it can be overwritten with command line
/// options.
fn get_sledstore_actual_path(gs: &mut GlobalState) -> &PathBuf {
    &gs.sledstore_dir_path
}

/// Gets version number, static if available, otherwise default.
fn get_version() -> &'static str {
    VERSION_O.unwrap_or(VERSION)
}

/// Gets Rust package name, static if available, otherwise default.
fn get_pkg_name() -> &'static str {
    PKG_NAME_O.unwrap_or(PKG_NAME)
}

/// Gets Rust package repository, static if available, otherwise default.
fn get_pkg_repository() -> &'static str {
    PKG_REPOSITORY_O.unwrap_or(PKG_REPOSITORY)
}

/// Gets program name without extension.
fn get_prog_without_ext() -> &'static str {
    get_pkg_name() // Todo: add "-rs" postfix
}

/// Gets timeout, argument-defined if available, otherwise default.
fn get_timeout(gs: &GlobalState) -> u64 {
    gs.ap.timeout.unwrap_or(TIMEOUT_DEFAULT)
}

/// Prints the version information
pub fn version() {
    println!("");
    println!(
        "  _|      _|      _|_|_|                     {}",
        get_prog_without_ext()
    );
    print!("  _|_|  _|_|    _|             _~^~^~_       ");
    println!("a rusty vision of a Matrix CLI client");
    println!(
        "  _|  _|  _|    _|         \\) /  o o  \\ (/   version {}",
        get_version()
    );
    println!(
        "  _|      _|    _|           '_   -   _'     repo {}",
        get_pkg_repository()
    );
    print!("  _|      _|      _|_|_|     / '-----' \\     ");
    println!("please submit PRs to make the vision a reality");
    println!("");
}

/// Asks the public for help
pub fn contribute() {
    println!("");
    println!(
        "This project is currently only a vision. The Python package {} exists. ",
        get_prog_without_ext()
    );
    println!("The vision is to have a compatible program in Rust. I cannot do it myself, ");
    println!("but I can coordinate and merge your pull requests. Have a look at the repo ");
    println!("{}. Please help! Please contribute ", get_pkg_repository());
    println!("code to make this vision a reality, and to one day have a functional ");
    println!("{} crate. Safe!", get_prog_without_ext());
}

/// If necessary reads homeserver name for login and puts it into the GlobalState.
/// If already set via --homeserver option, then it does nothing.
fn get_homeserver(gs: &mut GlobalState) {
    while gs.ap.homeserver.is_none() {
        print!("Enter your Matrix homeserver (e.g. https://some.homeserver.org): ");
        std::io::stdout()
            .flush()
            .expect("error: could not flush stdout");

        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .expect("error: unable to read user input");

        match input.trim().as_ref() {
            "" => {
                error!("Empty homeserver name is not allowed!");
            }
            // Todo: check format, e.g. starts with http, etc.
            _ => {
                gs.ap.homeserver = Url::parse(input.trim()).ok();
                if gs.ap.homeserver.is_none() {
                    error!(concat!(
                        "The syntax is incorrect. homeserver must be a URL! ",
                        "Start with 'http://' or 'https://'."
                    ));
                } else {
                    debug!("homeserver is {:?}", gs.ap.homeserver);
                }
            }
        }
    }
}

/// If necessary reads user name for login and puts it into the GlobalState.
/// If already set via --user-login option, then it does nothing.
fn get_user_login(gs: &mut GlobalState) {
    while gs.ap.user_login.is_none() {
        print!("Enter your full Matrix username (e.g. @john:some.homeserver.org): ");
        std::io::stdout()
            .flush()
            .expect("error; could not flush stdout");

        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .expect("error: unable to read user input");

        match input.trim().as_ref() {
            "" => {
                error!("Empty user name is not allowed!");
            }
            // Todo: check format, e.g. starts with letter, has @, has :, etc.
            _ => {
                gs.ap.user_login = Some(input.trim().to_string());
                debug!("user_login is {}", input);
            }
        }
    }
}

/// If necessary reads password for login and puts it into the GlobalState.
/// If already set via --password option, then it does nothing.
fn get_password(gs: &mut GlobalState) {
    while gs.ap.password.is_none() {
        print!("Enter your Matrix password: ");
        std::io::stdout()
            .flush()
            .expect("error: could not flush stdout");

        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .expect("error: unable to read user input");

        match input.trim().as_ref() {
            "" => {
                error!("Empty password is not allowed!");
            }
            // Todo: check format, e.g. starts with letter, has @, has :, etc.
            _ => {
                gs.ap.password = Some(input.trim().to_string());
                debug!("password is {}", input);
            }
        }
    }
}

/// If necessary reads device for login and puts it into the GlobalState.
/// If already set via --device option, then it does nothing.
fn get_device(gs: &mut GlobalState) {
    while gs.ap.device.is_none() {
        print!(
            concat!(
                "Enter your desired name for the Matrix device ",
                "that is going to be created for you (e.g. {}): "
            ),
            get_prog_without_ext()
        );
        std::io::stdout()
            .flush()
            .expect("error: could not flush stdout");

        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .expect("error: unable to read user input");

        match input.trim().as_ref() {
            "" => {
                error!("Empty device is not allowed!");
            }
            // Todo: check format, e.g. starts with letter, has @, has :, etc.
            _ => {
                gs.ap.device = Some(input.trim().to_string());
                debug!("device is {}", input);
            }
        }
    }
}

/// If necessary reads room_default for login and puts it into the GlobalState.
/// If already set via --room_default option, then it does nothing.
fn get_room_default(gs: &mut GlobalState) {
    while gs.ap.room_default.is_none() {
        print!(concat!(
            "Enter name of one of your Matrix rooms that you want to use as default room  ",
            "(e.g. !someRoomId:some.homeserver.org): "
        ));
        std::io::stdout()
            .flush()
            .expect("error: could not flush stdout");

        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .expect("error: unable to read user input");

        match input.trim().as_ref() {
            "" => {
                error!("Empty name of default room is not allowed!");
            }
            // Todo: check format, e.g. starts with letter, has @, has :, etc.
            _ => {
                gs.ap.room_default = Some(input.trim().to_string());
                debug!("room_default is {}", input);
            }
        }
    }
}

/// A room is either specified with --room or the default from credentials file is used
/// On error return None.
fn get_room(gs: &GlobalState) -> Option<String> {
    debug!("get_room() shows credentials {:?}", gs.credentials);
    let room = gs.ap.room.clone();
    let droom = match &gs.credentials {
        Some(inner) => Some(inner.room_default.clone()),
        None => None,
    };
    if room.is_none() && droom.is_none() {
        error!(
            "Error: InvalidRoom in get_room(). {:?} {:?}",
            gs.ap, gs.credentials
        );
        return None;
    }
    if room.is_none() {
        return droom;
    } else {
        return room;
    }
}

/// Return true if credentials file exists, false otherwise
fn credentials_exist(gs: &GlobalState) -> bool {
    let dp = get_credentials_default_path();
    let ap = get_credentials_actual_path(gs);
    debug!(
        "credentials_default_path = {:?}, credentials_actual_path = {:?}",
        dp, ap
    );
    let exists = ap.is_file();
    if exists {
        debug!("{:?} exists and is file. Not sure if readable though.", ap);
    } else {
        debug!("{:?} does not exist or is not a file.", ap);
    }
    exists
}

/// Return true if sledstore dir exists, false otherwise
#[allow(dead_code)]
fn sledstore_exist(gs: &mut GlobalState) -> bool {
    let dp = get_sledstore_default_path();
    let ap = get_sledstore_actual_path(gs);
    debug!(
        "sledstore_default_path = {:?}, sledstore_actual_path = {:?}",
        dp, ap
    );
    let exists = ap.is_dir();
    if exists {
        debug!(
            "{:?} exists and is directory. Not sure if readable though.",
            ap
        );
    } else {
        debug!("{:?} does not exist or is not a directory.", ap);
    }
    exists
}

/// Handle the --login CLI argument
pub(crate) async fn cli_login(gs: &mut GlobalState) -> Result<Client> {
    let login_type = gs.ap.login.as_ref().unwrap();
    if login_type != "password" && login_type != "sso" {
        error!(
            "Login option only supports 'password' and 'sso' as choice. {} is unknown.",
            login_type
        );
        return Err(Error::UnknownCliParameter);
    }
    if login_type == "sso" {
        error!("Login option 'sso' currently not supported. Use 'password' for the time being.");
        return Err(Error::UnsupportedCliParameter);
    }
    get_homeserver(gs);
    get_user_login(gs);
    get_password(gs);
    get_device(gs); // human-readable device name
    get_room_default(gs);
    info!(
        "Parameters for login are: {:?} {:?} {:?} {:?} {:?}",
        gs.ap.homeserver, gs.ap.user_login, gs.ap.password, gs.ap.device, gs.ap.room_default
    );
    if credentials_exist(gs) {
        error!(concat!(
            "Credentials file already exists. You have already logged in in ",
            "the past. No login needed. Skipping login. If you really want to log in ",
            "(i.e. create a new device), then logout first, or move credentials file manually. ",
            "Or just run your command again but without the '--login' option to log in ",
            "via your existing credentials and access token. ",
        ));
        return Err(Error::LoginUnnecessary);
    } else {
        let client = crate::login(
            gs,
            &gs.ap.homeserver.clone().ok_or(Error::MissingCliParameter)?,
            &gs.ap.user_login.clone().ok_or(Error::MissingCliParameter)?,
            &gs.ap.password.clone().ok_or(Error::MissingCliParameter)?,
            &gs.ap.device.clone().ok_or(Error::MissingCliParameter)?,
            &gs.ap
                .room_default
                .clone()
                .ok_or(Error::MissingCliParameter)?,
        )
        .await?;
        Ok(client)
    }
}

/// Attempt a restore-login iff the --login CLI argument is missing.
/// In other words try a re-login using the access token from the credentials file.
pub(crate) async fn cli_restore_login(gs: &mut GlobalState) -> Result<Client> {
    info!("restore_login implicitly chosen.");
    if !credentials_exist(gs) {
        error!(concat!(
            "Credentials file does not exists. Consider doing a '--logout' to clean up, ",
            "then perform a '--login'."
        ));
        return Err(Error::NotLoggedIn);
    } else {
        let client = crate::restore_login(gs).await?;
        debug!(
            "restore_login returned successfully, credentials are {:?}.",
            gs.credentials
        );
        Ok(client)
    }
}

/// Handle the --verify CLI argument
pub(crate) async fn cli_verify(clientres: &Result<Client>) -> Result {
    info!("Verify chosen.");
    return crate::verify(clientres).await;
}

/// Handle the --message CLI argument
pub(crate) async fn cli_message(clientres: &Result<Client>, gs: &GlobalState) -> Result {
    info!("Message chosen.");
    if gs.ap.message.is_none() {
        return Ok(()); // nothing to do
    }
    if clientres.as_ref().is_err() {
        return Ok(()); // nothing to do, this error has already been reported
    }
    let msg = gs.ap.message.as_ref().unwrap();
    if msg == "" {
        info!("Skipping empty text message.");
        return Ok(());
    };
    let fmsg = if msg == "-" {
        let mut line = String::new();
        if atty::is(Stream::Stdin) {
            print!("Message: ");
            std::io::stdout()
                .flush()
                .expect("error: could not flush stdout");
            io::stdin().read_line(&mut line)?;
        } else {
            io::stdin().read_to_string(&mut line)?;
        }
        line
    } else if msg == r"\-" {
        "-".to_string()
    } else {
        msg.to_string()
    };
    let roomstr = match get_room(gs) {
        Some(inner) => inner,
        _ => return Err(Error::InvalidRoom),
    };
    return message(
        clientres,
        fmsg,
        roomstr,
        gs.ap.code,
        gs.ap.markdown,
        gs.ap.notice,
        gs.ap.emote,
    )
    .await;
}

/// Handle the --file CLI argument
pub(crate) async fn cli_file(clientres: &Result<Client>, gs: &GlobalState) -> Result {
    info!("File chosen.");
    if gs.ap.file.is_none() {
        return Ok(()); // nothing to do
    }
    if clientres.as_ref().is_err() {
        return Ok(()); // nothing to do, this error has already been reported
    }
    let filename = gs.ap.file.as_ref().unwrap();
    if filename == "" {
        info!("Skipping empty file name.");
        return Ok(());
    };
    let roomstr = match get_room(gs) {
        Some(inner) => inner,
        _ => return Err(Error::InvalidRoom),
    };
    return file(
        clientres,
        PathBuf::from(filename),
        roomstr,
        None, // label, use default filename
        None, // mime, guess it
    )
    .await;
}

/// Handle the --devices CLI argument
pub(crate) async fn cli_devices(clientres: &Result<Client>) -> Result {
    info!("Devices chosen.");
    return crate::devices(clientres).await;
}

/// Handle the --logout CLI argument
pub(crate) async fn cli_logout(
    clientres: &Result<Client>,
    gs: &GlobalState,
    arg: String,
) -> Result {
    info!("Logout chosen.");
    if arg != "me" && arg != "all" {
        error!("Login option only supports 'me' and 'all' as choice.");
        return Err(Error::UnknownCliParameter);
    }
    if arg == "all" {
        error!("Logout option 'all' currently not supported. Use 'me' for the time being.");
        return Err(Error::UnsupportedCliParameter);
    }
    return crate::logout(clientres, gs).await;
}

/// We need your code contributions! Please add features and make PRs! :pray: :clap:
#[tokio::main]
async fn main() -> Result {
    let prog_desc: String;
    let verify_desc: String;
    let logout_desc: String;

    let mut gs: GlobalState = GlobalState::new("test".to_string());

    {
        // this block limits scope of borrows by ap.refer() method
        let mut ap = ArgumentParser::new();
        prog_desc = format!(
            concat!(
                "Welcome to {prog:?}, a Matrix CLI client. ─── ",
                "On first run use --login to log in, to authenticate. ",
                "On second run we suggest to use --verify to get verified. ",
                "Emoji verification is built-in which can be used ",
                "to verify devices. ",
                "On further runs this program implements a simple Matrix CLI ",
                "client that can send messages, verify devices, operate on rooms, ",
                "etc.  ───  ─── ",
                "This project is currently only a vision. The Python package {prog:?} ",
                "exists. The vision is to have a compatible program in Rust. I cannot ",
                "do it myself, but I can coordinate and merge your pull requests. ",
                "Have a look at the repo {repo:?}. Please help! Please contribute ",
                "code to make this vision a reality, and to one day have a functional ",
                "{prog:?} crate. Safe!",
            ),
            prog = get_prog_without_ext(),
            repo = get_pkg_repository()
        );
        ap.set_description(&prog_desc);
        ap.refer(&mut gs.ap.contribute).add_option(
            &["--contribute"],
            StoreTrue,
            "Please contribute.",
        );
        ap.refer(&mut gs.ap.version).add_option(
            &["-v", "--version"],
            StoreTrue,
            "Print version number.",
        );
        ap.refer(&mut gs.ap.debug).add_option(
            &["-d", "--debug"],
            IncrBy(1usize),
            concat!(
                "Overwrite the default log level. If not used, then the default ",
                "log level set with environment variable 'RUST_LOG' will be used. ",
                "If used, log level will be set to 'DEBUG' and debugging information ",
                "will be printed. ",
                "'-d' is a shortcut for '--log-level DEBUG'. ",
                "See also '--log-level'. '-d' takes precedence over '--log-level'. ",
                "Additionally, have a look also at the option '--verbose'. ",
            ),
        );
        ap.refer(&mut gs.ap.log_level).add_option(
            &["--log-level"],
            StoreOption,
            concat!(
                "Set the log level by overwriting the default log level. ",
                "If not used, then the default ",
                "log level set with environment variable 'RUST_LOG' will be used. ",
                "Possible values are ",
                "'DEBUG', 'INFO', 'WARN', and 'ERROR'. ",
                "See also '--debug' and '--verbose'.",
            ),
        );
        ap.refer(&mut gs.ap.verbose).add_option(
            &["--verbose"],
            IncrBy(1usize),
            concat!(
                "Set the verbosity level. If not used, then verbosity will be ",
                "set to low. If used once, verbosity will be high. ",
                "If used more than once, verbosity will be very high. ",
                "Verbosity only affects the debug information. ",
                "So, if '--debug' is not used then '--verbose' will be ignored.",
            ),
        );
        ap.refer(&mut gs.ap.login).add_option(
            &["--login"],
            StoreOption,
            concat!(
                "Login to and authenticate with the Matrix homeserver. ",
                "This requires exactly one argument, the login method. ",
                "Currently two choices are offered: 'password' and 'sso'. ",
                "Provide one of these methods. ",
                "If you have chosen 'password', ",
                "you will authenticate through your account password. You can ",
                "optionally provide these additional arguments: ",
                "--homeserver to specify the Matrix homeserver, ",
                "--user-login to specify the log in user id, ",
                "--password to specify the password, ",
                "--device to specify a device name, ",
                "--room-default to specify a default room for sending/listening. ",
                "If you have chosen 'sso', ",
                "you will authenticate through Single Sign-On. A web-browser will ",
                "be started and you authenticate on the webpage. You can ",
                "optionally provide these additional arguments: ",
                "--homeserver to specify the Matrix homeserver, ",
                "--user-login to specify the log in user id, ",
                "--device to specify a device name, ",
                "--room-default to specify a default room for sending/listening. ",
                "See all the extra arguments for further explanations. ----- ",
                "SSO (Single Sign-On) starts a web ",
                "browser and connects the user to a web page on the ",
                "server for login. SSO will only work if the server ",
                "supports it and if there is access to a browser. So, don't use SSO ",
                "on headless homeservers where there is no ",
                "browser installed or accessible.",
            ),
        );
        verify_desc = format!(
            concat!(
                "Perform verification. By default, no ",
                "verification is performed. ",
                "Verification is done via Emojis. ",
                "If verification is desired, run this program in the ",
                "foreground (not as a service) and without a pipe. ",
                "While verification is optional it is highly recommended, and it ",
                "is recommended to be done right after (or together with) the ",
                "--login action. Verification is always interactive, i.e. it ",
                "required keyboard input. ",
                "Verification questions ",
                "will be printed on stdout and the user has to respond ",
                "via the keyboard to accept or reject verification. ",
                "Once verification is complete, the program may be ",
                "run as a service. Verification is best done as follows: ",
                "Perform a cross-device verification, that means, perform a ",
                "verification between two devices of the *same* user. For that, ",
                "open (e.g.) Element in a browser, make sure Element is using the ",
                "same user account as the {prog} user (specified with ",
                "--user-login at --login). Now in the Element webpage go to the room ",
                "that is the {prog} default room (specified with ",
                "--room-default at --login). OK, in the web-browser you are now the ",
                "same user and in the same room as {prog}. ",
                "Now click the round 'i' 'Room Info' icon, then click 'People', ",
                "click the appropriate user (the {prog} user), ",
                "click red 'Not Trusted' text ",
                "which indicated an untrusted device, then click the square ",
                "'Interactively verify by Emoji' button (one of 3 button choices). ",
                "At this point both web-page and {prog} in terminal ",
                "show a set of emoji icons and names. Compare them visually. ",
                "Confirm on both sides (Yes, They Match, Got it), finally click OK. ",
                "You should see a green shield and also see that the ",
                "{prog} device is now green and verified in the webpage. ",
                "In the terminal you should see a text message indicating success. ",
                "You should now be verified across all devices and across all users.",
            ),
            prog = get_prog_without_ext()
        );
        ap.refer(&mut gs.ap.verify)
            .add_option(&["--verify"], StoreTrue, &verify_desc);

        ap.refer(&mut gs.ap.message).add_option(
            &["-m", "--message"],
            StoreOption,
            concat!(
                // "Send this message. Message data must not be binary data, it ",
                // "must be text. If no '-m' is used and no other conflicting ",
                // "arguments are provided, and information is piped into the program, ",
                // "then the piped data will be used as message. ",
                // "Finally, if there are no operations at all in the arguments, then ",
                // "a message will be read from stdin, i.e. from the keyboard. ",
                // "This option can be used multiple times to send ",
                // "multiple messages. If there is data piped ",
                // "into this program, then first data from the ",
                // "pipe is published, then messages from this ",
                // "option are published. Messages will be sent last, ",
                // "i.e. after objects like images, audio, files, events, etc. ",
                // "Input piped via stdin can additionally be specified with the ",
                // "special character '-'. ",
                // "If you want to feed a text message into the program ",
                // "via a pipe, via stdin, then specify the special ",
                // "character '-'. If '-' is specified as message, ",
                // "then the program will read the message from stdin. ",
                // "If your message is literally '-' then use '\\-' ",
                // "as message in the argument. ",
                // "'-' may appear in any position, i.e. '-m \"start\" - \"end\"' ",
                // "will send 3 messages out of which the second one is read from stdin. ",
                // "'-' may appear only once overall in all arguments. ",
                "Send this message. Message data must not be binary data, it ",
                "must be text. ",
                "Input piped via stdin can additionally be specified with the ",
                "special character '-'. ",
                "If you want to feed a text message into the program ",
                "via a pipe, via stdin, then specify the special ",
                "character '-'. If '-' is specified as message, ",
                "then the program will read the message from stdin. ",
                "If your message is literally '-' then use '\\-' ",
                "as message in the argument. In some shells this needs to be ",
                "escaped requiring a '\\-'. If you want to read the message from ",
                "the keyboard use '-' and do not pipe anything into stdin, then ",
                "a message will be requested and read from the keyboard. ",
                "Keyboard input is limited to one line. ",
            ),
        );
        logout_desc = format!(
            concat!(
                "Logout this or all devices from the Matrix homeserver. ",
                "This requires exactly one argument. ",
                "Two choices are offered: 'me' and 'all'. ",
                "Provide one of these choices. ",
                "If you choose 'me', only the one device {prog} ",
                "is currently using will be logged out. ",
                "If you choose 'all', all devices of the user used by ",
                "{prog} will be logged out. ",
                "While --logout neither removes the credentials nor the store, the ",
                "logout action removes the device and makes the access-token stored ",
                "in the credentials invalid. Hence, after a --logout, one must ",
                "manually remove creditials and store, and then perform a new ",
                "--login to use {prog} again. ",
                "You can perfectly use ",
                "{prog} without ever logging out. --logout is a cleanup ",
                "if you have decided not to use this (or all) device(s) ever again.",
            ),
            prog = get_prog_without_ext()
        );
        ap.refer(&mut gs.ap.logout)
            .add_option(&["--logout"], StoreOption, &logout_desc);
        ap.refer(&mut gs.ap.homeserver).add_option(
            &["--homeserver"],
            StoreOption,
            concat!(
                "Specify a homeserver for use by certain actions. ",
                "It is an optional argument. ",
                "By default --homeserver is ignored and not used. ",
                "It is used by '--login' action. ",
                "If not provided for --login the user will be queried via keyboard.",
            ),
        );

        ap.refer(&mut gs.ap.user_login).add_option(
            &["--user-login"], // @john:example.com and @john and john accepted
            StoreOption,
            concat!(
                "Optional argument to specify the user for --login. ",
                "This gives the otion to specify the user id for login. ",
                "For '--login sso' the --user-login is not needed as user id can be ",
                "obtained from server via SSO. For '--login password', if not ",
                "provided it will be queried via keyboard. A full user id like ",
                "'@john:example.com', a partial user name like '@john', and ",
                "a short user name like 'john' can be given. ",
                "--user-login is only used by --login and ignored by all other ",
                "actions.",
            ),
        );

        ap.refer(&mut gs.ap.password).add_option(
            &["--password"],
            StoreOption,
            concat!(
                "Specify a password for use by certain actions. ",
                "It is an optional argument. ",
                "By default --password is ignored and not used. ",
                "It is used by '--login password' and '--delete-device' ",
                "actions. ",
                "If not provided for --login the user will be queried via keyboard.",
            ),
        );

        ap.refer(&mut gs.ap.device).add_option(
            &["--device"],
            StoreOption,
            concat!(
                "Specify a device name, for use by certain actions. ",
                "It is an optional argument. ",
                "By default --device is ignored and not used. ",
                "It is used by '--login' action. ",
                "If not provided for --login the user will be queried via keyboard. ",
                "If you want the default value specify ''. ",
                "Multiple devices (with different device id) may have the same device ",
                "name. In short, the same device name can be assigned to multiple ",
                "different devices if desired.",
            ),
        );

        ap.refer(&mut gs.ap.room_default).add_option(
            &["--room-default"],
            StoreOption,
            concat!(
                "Optionally specify a room as the ",
                "default room for future actions. If not specified for --login, it ",
                "will be queried via the keyboard. --login stores the specified room ",
                "as default room in your credentials file. This option is only used ",
                "in combination with --login. A default room is needed. Specify a ",
                "valid room either with --room-default or provide it via keyboard.",
            ),
        );

        ap.refer(&mut gs.ap.devices).add_option(
            &["--devices"],
            StoreTrue,
            concat!(
                "Print the list of devices. All device of this ",
                "account will be printed, one device per line.",
            ),
        );

        ap.refer(&mut gs.ap.timeout).add_option(
            &["--timeout"],
            StoreOption,
            concat!(
                "Set the timeout of the calls to the Matrix server. ",
                "By default they are set to 60 seconds. ",
                "Specify the timeout in seconds. Use 0 for infinite timeout. ",
            ),
        );

        ap.refer(&mut gs.ap.markdown).add_option(
            &["--markdown"],
            StoreTrue,
            concat!(
                "There are 3 message formats for '--message'. ",
                "Plain text, MarkDown, and Code. By default, if no ",
                "command line options are specified, 'plain text' ",
                "will be used. Use '--markdown' or '--code' to set ",
                "the format to MarkDown or Code respectively. ",
                "'--markdown' allows sending of text ",
                "formatted in MarkDown language. '--code' allows ",
                "sending of text as a Code block.",
            ),
        );

        ap.refer(&mut gs.ap.code).add_option(
            &["--code"],
            StoreTrue,
            concat!(
                "There are 3 message formats for '--message'. ",
                "Plain text, MarkDown, and Code. By default, if no ",
                "command line options are specified, 'plain text' ",
                "will be used. Use '--markdown' or '--code' to set ",
                "the format to MarkDown or Code respectively. ",
                "'--markdown' allows sending of text ",
                "formatted in MarkDown language. '--code' allows ",
                "sending of text as a Code block.",
            ),
        );

        ap.refer(&mut gs.ap.room).add_option(
            &["-r", "--room"],
            StoreOption,
            concat!(
                // "Optionally specify one or multiple rooms via room ids or ",
                // "room aliases. --room is used by various send actions and ",
                // "various listen actions. ",
                // "The default room is provided ",
                // "in the credentials file (specified at --login with --room-default). ",
                // "If a room (or multiple ones) ",
                // "is (or are) provided in the --room arguments, then it ",
                // "(or they) will be used ",
                // "instead of the one from the credentials file. ",
                // "The user must have access to the specified room ",
                // "in order to send messages there or listen on the room. ",
                // "Messages cannot ",
                // "be sent to arbitrary rooms. When specifying the ",
                // "room id some shells require the exclamation mark ",
                // "to be escaped with a backslash. ",
                // "As an alternative to specifying a room as destination, ",
                // "one can specify a user as a destination with the '--user' ",
                // "argument. See '--user' and the term 'DM (direct messaging)' ",
                // "for details. Specifying a room is always faster and more ",
                // "efficient than specifying a user. Not all listen operations ",
                // "allow setting a room. Read more under the --listen options ",
                // "and similar. Most actions also support room aliases instead of ",
                // "room ids. Some even short room aliases.",
                "Optionally specify a room by room id. '--room' is used by ",
                "by various options like '--message'. If no '--room' is ",
                "provided the default room from the credentials file will be ",
                "used. ",
                "If a room is provided in the '--room' argument, then it ",
                "will be used ",
                "instead of the one from the credentials file. ",
                "The user must have access to the specified room ",
                "in order to send messages there or listen on the room. ",
                "Messages cannot ",
                "be sent to arbitrary rooms. When specifying the ",
                "room id some shells require the exclamation mark ",
                "to be escaped with a backslash. ",
            ),
        );

        ap.refer(&mut gs.ap.file).add_option(
            &["-f", "--file"],
            StoreOption,
            concat!(
                // "Send this file (e.g. PDF, DOC, MP4). "
                // "This option can be used multiple times to send "
                // "multiple files. First files are sent, "
                // "then text messages are sent. "
                // "If you want to feed a file into {PROG_WITHOUT_EXT} "
                // "via a pipe, via stdin, then specify the special "
                // "character '-'. See description of '-i' to see how '-' is handled.",
                "Send this file (e.g. PDF, DOC, MP4). ",
                "First files are sent, ",
                "then text messages are sent. ",
            ),
        );

        ap.refer(&mut gs.ap.notice).add_option(
            &["--notice"],
            StoreTrue,
            concat!(
                "There are 3 message types for '--message'. ",
                "Text, Notice, and Emote. By default, if no ",
                "command line options are specified, 'Text' ",
                "will be used. Use '--notice' or '--emote' to set ",
                "the type to Notice or Emote respectively. ",
                "'--notice' allows sending of text ",
                "as a notice. '--emote' allows ",
                "sending of text as an emote.",
            ),
        );

        ap.refer(&mut gs.ap.emote).add_option(
            &["--emote"],
            StoreTrue,
            concat!(
                "There are 3 message types for '--message'. ",
                "Text, Notice, and Emote. By default, if no ",
                "command line options are specified, 'Text' ",
                "will be used. Use '--notice' or '--emote' to set ",
                "the type to Notice or Emote respectively. ",
                "'--notice' allows sending of text ",
                "as a notice. '--emote' allows ",
                "sending of text as an emote.",
            ),
        );

        ap.parse_args_or_exit();
    }

    // handle log level and debug options
    let env_org_rust_log = env::var("RUST_LOG")
        .unwrap_or("".to_string())
        .to_uppercase();
    if gs.ap.debug > 0 {
        // -d overwrites --log-level
        gs.ap.log_level = Some("DEBUG".to_string())
    }
    if gs.ap.log_level.is_some() {
        let ll = gs.ap.log_level.clone().unwrap();
        if ll != "DEBUG" && ll != "INFO" && ll != "WARN" && ll != "ERROR" {
            error!("Log-level option only supports 'DEBUG', 'INFO', 'WARN', or 'ERROR' as choice.");
        }
        // overwrite environment variable
        env::set_var("RUST_LOG", &ll);
    } else {
        gs.ap.log_level = Some(env_org_rust_log.clone())
    }
    // set log level e.g. via RUST_LOG=DEBUG cargo run, use newly set venv var value
    tracing_subscriber::fmt::init();
    debug!("Original RUST_LOG env var is {}", env_org_rust_log);
    debug!(
        "Final RUST_LOG env var is {}",
        env::var("RUST_LOG")
            .unwrap_or("".to_string())
            .to_uppercase()
    );
    debug!("Final log_level option is {:?}", gs.ap.log_level);
    if enabled!(Level::TRACE) {
        debug!("Log level is set to TRACE.");
    } else if enabled!(Level::DEBUG) {
        debug!("Log level is set to DEBUG.");
    }
    debug!("Version is {}", get_version());
    debug!("Package name is {}", get_pkg_name());
    debug!("Repo is {}", get_pkg_repository());
    debug!("contribute flag is {}", gs.ap.contribute);
    debug!("version flag is set to {}", gs.ap.version);
    debug!("debug flag is {}", gs.ap.debug);
    match gs.ap.log_level {
        Some(inner) => {
            gs.ap.log_level = Some(inner.trim().to_lowercase());
        }
        _ => (),
    }
    debug!("log_level option is {:?}", gs.ap.log_level);
    debug!("verbose option is {}", gs.ap.verbose);
    match gs.ap.login {
        Some(inner) => {
            gs.ap.login = Some(inner.trim().to_lowercase());
        }
        _ => (),
    }
    debug!("login option is {:?}", gs.ap.login);
    debug!("verify flag is {:?}", gs.ap.verify);
    debug!("message option is {:?}", gs.ap.message);
    match gs.ap.logout {
        Some(inner) => {
            gs.ap.logout = Some(inner.trim().to_lowercase());
        }
        _ => (),
    }
    debug!("logout option is {:?}", gs.ap.logout);
    debug!("homeserver option is {:?}", gs.ap.homeserver);
    debug!("user_login option is {:?}", gs.ap.user_login);
    debug!("password option is {:?}", gs.ap.password);
    debug!("device option is {:?}", gs.ap.device);
    debug!("room_default option is {:?}", gs.ap.room_default);
    debug!("devices flag is {:?}", gs.ap.devices);
    debug!("timeout option is {:?}", gs.ap.timeout);
    debug!("markdown flag is {:?}", gs.ap.markdown);
    debug!("code flag is {:?}", gs.ap.code);
    debug!("room option is {:?}", gs.ap.room);
    debug!("file option is {:?}", gs.ap.file);
    debug!("notice flag is {:?}", gs.ap.notice);
    debug!("emote flag is {:?}", gs.ap.emote);

    // Todo : make all option args lower case
    if gs.ap.version {
        crate::version();
    };
    if gs.ap.contribute {
        crate::contribute();
    };
    let clientres = if gs.ap.login.is_some() {
        crate::cli_login(&mut gs).await
    } else {
        crate::cli_restore_login(&mut gs).await
    };
    match clientres {
        Ok(ref _n) => {
            debug!("A valid client connection has been established.");
        }
        Err(ref e) => {
            info!(
                "Most operations will be skipped because you don't have a valid client connection."
            );
            error!("Error: {}", e);
            // don't quit yet, e.g. logout can still do stuff;
            // return Err(Error::LoginFailed);
        }
    };
    let gsclone = gs.clone();
    if gsclone.ap.verify && clientres.as_ref().is_ok() {
        match crate::cli_verify(&clientres).await {
            Ok(ref _n) => debug!("crate::verify successful"),
            Err(ref e) => error!("Error: crate::verify reported {}", e),
        };
    };

    if gsclone.ap.devices && clientres.as_ref().is_ok() {
        match crate::cli_devices(&clientres).await {
            Ok(ref _n) => debug!("crate::message successful"),
            Err(ref e) => error!("Error: crate::message reported {}", e),
        };
    };

    // send text message
    if gsclone.ap.message.is_some() && clientres.as_ref().is_ok() {
        match crate::cli_message(&clientres, &gsclone).await {
            Ok(ref _n) => debug!("crate::message successful"),
            Err(ref e) => error!("Error: crate::message reported {}", e),
        };
    };

    // send file
    if gsclone.ap.file.is_some() && clientres.as_ref().is_ok() {
        match crate::cli_file(&clientres, &gsclone).await {
            Ok(ref _n) => debug!("crate::file successful"),
            Err(ref e) => error!("Error: crate::file reported {}", e),
        };
    };

    if gsclone.ap.logout.is_some() {
        match crate::cli_logout(
            &clientres,
            &gsclone,
            gsclone.ap.logout.as_ref().unwrap().to_string(),
        )
        .await
        {
            Ok(ref _n) => debug!("crate::logout successful"),
            Err(ref e) => error!("Error: crate::verify reported {}", e),
        };
    };
    debug!("Good bye");
    Ok(())
}

/// Future test cases will be put here
#[cfg(test)]
mod tests {
    // Note this useful idiom: importing names from outer (for mod tests) scope.
    use super::*;

    #[test]
    fn test_version() {
        assert_eq!(version(), ());
    }

    #[test]
    fn test_contribute() {
        assert_eq!(contribute(), ());
    }
}
