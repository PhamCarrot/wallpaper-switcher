use super::*;
use std::os::windows::process::ExitStatusExt;
use windows_sys::Win32::Foundation::{WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows_sys::Win32::System::Threading::{
    OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE,
};

const CHILD: &str = "WALLPAPER_SWITCHER_CONSOLE_TEST_ROLE";
const WITNESS: &str = "WALLPAPER_SWITCHER_CONSOLE_TEST_WITNESS";
const PATH: &str = r"J:\owned Ω\paper 顏.png";
const DAMAGED_PATH: &str = r"J:\owned\3197745768\?????????3.23_x264_x264_Mux.mp4";

fn fixture(role: &str) -> Command {
    let mut cmd = Command::new(std::env::current_exe().unwrap());
    cmd.args([
        "--exact",
        "engine_ipc::query_console::tests::console_child_fixture",
        "--nocapture",
        "--test-threads=1",
    ])
    .creation_flags(DETACHED_PROCESS)
    .env(CHILD, role);
    cmd
}

#[test]
fn console_child_fixture() {
    let Ok(role) = std::env::var(CHILD) else {
        return;
    };
    if role == "waiting-writer" {
        std::fs::write(
            std::env::var_os(WITNESS).unwrap(),
            std::process::id().to_string(),
        )
        .unwrap();
        thread::sleep(Duration::from_secs(20));
        return;
    }
    if let Some(value) = role.strip_prefix("write:") {
        for handle in [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
            unsafe {
                SetStdHandle(handle, std::ptr::null_mut());
            }
        }
        assert_ne!(unsafe { AttachConsole(ATTACH_PARENT_PROCESS) }, 0);
        let screen = OpenOptions::new()
            .read(true)
            .write(true)
            .open("CONOUT$")
            .unwrap();
        let chars: Vec<_> = value.encode_utf16().collect();
        if !chars.is_empty() {
            let mut written = 0;
            assert_ne!(
                unsafe {
                    WriteConsoleW(
                        screen.as_raw_handle(),
                        chars.as_ptr(),
                        chars.len() as u32,
                        &mut written,
                        std::ptr::null(),
                    )
                },
                0
            );
            assert_eq!(written as usize, chars.len());
        }
        std::process::exit(0);
    }
    wait_for_release(Instant::now() + Duration::from_secs(5)).expect("fixture job barrier");
    let ok = std::panic::catch_unwind(|| {
        let deadline = Instant::now() + Duration::from_secs(5);
        let screen = private_screen(deadline).expect("private hidden console");
        if role == "hold" {
            let _ = run_command_bounded(
                &mut fixture("waiting-writer"),
                Duration::from_secs(10),
                "owned waiting query",
            );
            panic!("parent should terminate the helper after the query starts");
        }
        let value = match role.as_str() {
            "unicode" => PATH,
            "emoji" => r"J:\owned 顏🙂.png",
            "empty" => "",
            "damaged" => DAMAGED_PATH,
            "multiline" => "J:\\one.png\r\nJ:\\two.png",
            _ => panic!("unknown fixture role"),
        };
        let output = run_command_bounded(
            &mut fixture(&format!("write:{value}")),
            deadline.saturating_duration_since(Instant::now()),
            "owned console writer",
        )
        .unwrap();
        assert!(output.status.success());
        // libtest emits its own header on stdout. The actual wallpaper was
        // written exclusively to the isolated console.
        assert!(!output_text(&output.stdout).contains(PATH));
        if role == "multiline" {
            assert!(screen_text(&screen).is_err());
        } else if role == "damaged" {
            let captured = screen_text(&screen).unwrap();
            assert_eq!(captured, value);
            let clean = ChildRunOutput {
                stdout: CapturedStream {
                    bytes: vec![],
                    discarded_bytes: 0,
                },
                ..output
            };
            let reply = combine_output(&clean, captured).unwrap();
            let wire = serde_json::to_vec(&reply).unwrap();
            let decoded: Reply = serde_json::from_slice(&wire).unwrap();
            assert_eq!(
                decoded.path,
                QueryPath::Rejected("Engine wallpaper path contains literal question marks".into())
            );
        } else if role == "emoji" {
            let captured = screen_text(&screen).unwrap();
            if captured != value {
                assert!(
                    captured.contains('\u{fffd}'),
                    "silent filename corruption: {captured:?}"
                );
                let clean = ChildRunOutput {
                    stdout: CapturedStream {
                        bytes: vec![],
                        discarded_bytes: 0,
                    },
                    ..output
                };
                assert!(combine_output(&clean, captured).is_err());
            }
        } else {
            assert_eq!(screen_text(&screen).unwrap(), value);
        }
    })
    .is_ok();
    std::process::exit(if ok { 0 } else { 101 })
}

#[test]
fn provider_private_console_recovers_unicode_and_rejects_multiple_lines() {
    for role in ["unicode", "emoji", "empty", "damaged", "multiline"] {
        let result = run_in_query_job(&mut fixture(role), Duration::from_secs(8)).unwrap();
        assert!(
            result.status.success(),
            "{role}: {}",
            output_text(&result.stderr)
        );
    }
}

#[test]
fn provider_query_never_accepts_damaged_or_conflicting_channels() {
    let mut result = ChildRunOutput {
        process_id: 7,
        status: ExitStatus::from_raw(0),
        stdout: CapturedStream {
            bytes: Vec::new(),
            discarded_bytes: 0,
        },
        stderr: CapturedStream {
            bytes: Vec::new(),
            discarded_bytes: 0,
        },
    };
    assert_eq!(
        combine_output(&result, PATH.into()).unwrap().path,
        QueryPath::Path(PATH.into())
    );
    assert!(combine_output(&result, r"J:\damaged�.png".into()).is_err());
    result.stdout.bytes = b"J:/other.png\r\n".to_vec();
    assert!(combine_output(&result, PATH.into()).is_err());
    assert_eq!(
        combine_output(&result, String::new()).unwrap().path,
        QueryPath::Path("J:/other.png".into())
    );
    result.stdout.bytes = vec![0xff];
    assert!(combine_output(&result, String::new()).is_err());
    result.stdout.bytes = PATH.as_bytes().to_vec();
    result.stdout.discarded_bytes = 1;
    assert!(combine_output(&result, String::new()).is_err());
}

#[test]
fn provider_query_helper_exit_reaps_its_running_query() {
    let path = std::env::temp_dir().join(format!(
        "console-child-{}-{}.txt",
        std::process::id(),
        unsafe { GetTickCount64() }
    ));
    assert!(!path.exists());
    let job = QueryJob::new().unwrap();
    let mut helper = fixture("hold")
        .env(WITNESS, &path)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    job.release_helper(&mut helper).unwrap();
    let until = Instant::now() + Duration::from_secs(5);
    let pid = loop {
        if let Some(pid) = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| s.parse::<u32>().ok())
        {
            break pid;
        }
        assert!(
            Instant::now() < until && helper.try_wait().unwrap().is_none(),
            "owned child did not start"
        );
        thread::sleep(Duration::from_millis(5));
    };
    let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
    assert!(!handle.is_null());
    let query = unsafe { OwnedHandle::from_raw_handle(handle) };
    assert_eq!(
        unsafe { WaitForSingleObject(query.as_raw_handle(), 0) },
        WAIT_TIMEOUT
    );
    assert_eq!(
        stop_owned_child(&mut helper, Instant::now() + Duration::from_secs(2)),
        Some(true)
    );
    assert!(job.settle(Instant::now() + Duration::from_secs(2)).unwrap());
    assert!(job.is_empty().unwrap());
    assert_eq!(
        unsafe { WaitForSingleObject(query.as_raw_handle(), 2000) },
        WAIT_OBJECT_0
    );
    std::fs::remove_file(path).unwrap();
}
