#![cfg(feature = "shell")]
use ceh_browser::learning::{LearningControl, LearningForm, LearningOption, LearningPage};
use ceh_core::profiles::ProfileDraft;
use ceh_desktop::learning_recorder::LearningRecorder;
use serde_json::Value;
use std::{path::Path, sync::Mutex};

static ENV: Mutex<()> = Mutex::new(());

fn profile() -> ProfileDraft {
    ProfileDraft {
        id: "p".into(), label: "p".into(), enabled: true, procedure: "huellas".into(), province: "Madrid".into(),
        office: String::new(), document_type: "NIE".into(), document_number: "Y1234567X".into(),
        full_name: "ANA GARCIA PEREZ".into(), nationality: "RUSIA".into(), phone: "600123123".into(),
        email: "ana@example.test".into(), access_mode: "certificate".into(), clave_method: "movil".into(),
        fill_mode: "manual".into(), no_slots_mode: "salir".into(), applicant_no_slots_mode: "wait".into(),
        subprocedure: None, timezone: "Europe/Madrid".into(), browser_session: "persistent".into(), browser: "firefox".into(),
    }
}
fn page(path: &str, options: usize, text: &[&str]) -> LearningPage {
    page_at(path, options, text, "L0")
}
fn page_at(path: &str, options: usize, text: &[&str], load: &str) -> LearningPage {
    LearningPage {
        path: path.into(), load: load.into(), title: "Cita previa".into(), headings: vec![], text: text.iter().map(|t| t.to_string()).collect(),
        forms: vec![LearningForm { id: String::new(), name: String::new(), action: "acInfo".into(), method: "post".into() }],
        controls: vec![LearningControl {
            tag: "select".into(), kind: "select".into(), id: "tramiteGrupo[0]".into(), name: "tramiteGrupo[0]".into(),
            label: "Trámite".into(), form: "acInfo".into(), disabled: false,
            options: (0..options).map(|i| LearningOption { value: i.to_string(), label: format!("option {i}"), selected: false }).collect(),
        }],
    }
}
fn files(dir: &Path) -> Vec<String> {
    let mut names: Vec<_> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    names.sort();
    names
}
fn run_dir(root: &Path) -> std::path::PathBuf {
    std::fs::read_dir(root).unwrap().next().unwrap().unwrap().path()
}

#[test]
fn without_the_variable_there_is_no_recorder() {
    let _guard = ENV.lock().unwrap();
    unsafe { std::env::remove_var("CEH_LEARNING") };
    assert!(LearningRecorder::from_env(&profile()).is_none());
}

#[test]
fn a_new_page_is_written_and_a_repeat_of_the_same_document_is_not() {
    let root = tempfile::tempdir().unwrap();
    let mut recorder = LearningRecorder::create(root.path(), "run", &profile()).unwrap();
    recorder.record(page_at("/icpplustiem/selectSede", 3, &[], "doc-a"));
    recorder.record(page_at("/icpplustiem/selectSede", 3, &[], "doc-a"));
    let dir = run_dir(root.path());
    assert_eq!(files(&dir), ["0001.json", "path.jsonl"]);
    let first: Value = serde_json::from_str(&std::fs::read_to_string(dir.join("0001.json")).unwrap()).unwrap();
    assert_eq!(first["index"], 1);
    assert_eq!(first["page"]["path"], "/icpplustiem/selectSede");
    assert_eq!(std::fs::read_to_string(dir.join("path.jsonl")).unwrap().lines().count(), 1);
}

#[test]
fn a_changed_select_on_the_same_document_is_written_again_with_a_transition() {
    let root = tempfile::tempdir().unwrap();
    let mut recorder = LearningRecorder::create(root.path(), "run", &profile()).unwrap();
    recorder.record(page_at("/icpplustiem/selectSede", 1, &[], "doc-a"));
    recorder.record(page_at("/icpplustiem/selectSede", 3, &[], "doc-a"));
    let dir = run_dir(root.path());
    assert_eq!(files(&dir), ["0001.json", "0002.json", "path.jsonl"]);
    let lines: Vec<Value> = std::fs::read_to_string(dir.join("path.jsonl")).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    assert_eq!((lines[1]["from"].as_u64(), lines[1]["to"].as_u64()), (Some(1), Some(2)));
    assert_eq!(lines[1]["via_forms"], serde_json::json!(["acInfo"]));
}

#[test]
fn a_refresh_with_an_unchanged_structure_is_a_new_document_and_is_written() {
    let root = tempfile::tempdir().unwrap();
    let mut recorder = LearningRecorder::create(root.path(), "run", &profile()).unwrap();
    recorder.record(page_at("/icpplustiem/acCitar", 0, &["No hay citas disponibles"], "doc-a"));
    recorder.record(page_at("/icpplustiem/acCitar", 0, &["No hay citas disponibles"], "doc-b"));
    assert_eq!(files(&run_dir(root.path())), ["0001.json", "0002.json", "path.jsonl"]);
}

#[test]
fn profile_values_and_nie_shapes_never_reach_the_files() {
    let root = tempfile::tempdir().unwrap();
    let mut recorder = LearningRecorder::create(root.path(), "run", &profile()).unwrap();
    let mut shown = page("/icpplustiem/acVerifica", 1, &[
        "Nombre: ANA GARCIA PEREZ", "NIE Y1234567X", "Teléfono 600123123", "Correo ana@example.test", "Su NIE es X1234567L",
    ]);
    shown.title = "Datos de ANA GARCIA PEREZ".into();
    recorder.record(shown);
    for entry in std::fs::read_dir(run_dir(root.path())).unwrap() {
        let text = std::fs::read_to_string(entry.unwrap().path()).unwrap();
        for secret in ["ANA GARCIA PEREZ", "Y1234567X", "600123123", "ana@example.test", "X1234567L"] {
            assert!(!text.contains(secret), "{secret} reached a file");
        }
    }
}

#[test]
fn an_unwritable_directory_does_not_panic_and_stops_recording() {
    let root = tempfile::tempdir().unwrap();
    let mut recorder = LearningRecorder::create(root.path(), "run", &profile()).unwrap();
    std::fs::remove_dir_all(run_dir(root.path())).unwrap();
    recorder.record(page_at("/icpplustiem/a", 1, &[], "doc-a"));
    recorder.record(page_at("/icpplustiem/b", 1, &[], "doc-b"));
}

#[cfg(unix)]
#[test]
fn the_files_are_private() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    let mut recorder = LearningRecorder::create(root.path(), "run", &profile()).unwrap();
    recorder.record(page_at("/icpplustiem/a", 1, &[], "doc-a"));
    let dir = run_dir(root.path());
    assert_eq!(std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777, 0o700);
    assert_eq!(std::fs::metadata(dir.join("0001.json")).unwrap().permissions().mode() & 0o777, 0o600);
    assert_eq!(std::fs::metadata(dir.join("path.jsonl")).unwrap().permissions().mode() & 0o777, 0o600);
}

fn secret_free(text: &[&str], title: &str) {
    let root = tempfile::tempdir().unwrap();
    let mut recorder = LearningRecorder::create(root.path(), "run", &profile()).unwrap();
    let mut shown = page("/icpplustiem/acVerifica", 1, text);
    shown.title = title.into();
    recorder.record(shown);
    for entry in std::fs::read_dir(run_dir(root.path())).unwrap() {
        let written = std::fs::read_to_string(entry.unwrap().path()).unwrap();
        for secret in ["garcia", "GARCIA", "Garc", "perez", "PEREZ", "600123123", "600 123 123", "+34600123123", "600.123.123",
                       "ana@example.test", "ANA@EXAMPLE.TEST", "Y1234567X", "Y-1234567-X", "y 1234567 x", "12345678Z", "maria@gmail.com"] {
            assert!(!written.contains(secret), "{secret} reached a file");
        }
    }
}
#[test]
fn a_profile_value_shown_in_another_case_or_without_accents_is_redacted() {
    secret_free(&["Nombre: Ana García Pérez", "NOMBRE: ana garcia perez", "Garcia, Ana"], "Datos de ANA GARCIA");
}
#[test]
fn a_phone_in_another_format_is_redacted() {
    secret_free(&["Teléfono 600 123 123", "Tel. +34600123123", "Móvil 600.123.123"], "t");
}
#[test]
fn any_email_a_dni_and_a_nie_with_separators_are_redacted_even_when_not_in_the_profile() {
    secret_free(&["El correo maria@gmail.com no es válido", "ANA@EXAMPLE.TEST", "DNI 12345678Z", "NIE Y-1234567-X", "NIE y 1234567 x"], "t");
}
#[test]
fn ordinary_site_text_survives_the_redaction() {
    let root = tempfile::tempdir().unwrap();
    let mut recorder = LearningRecorder::create(root.path(), "run", &profile()).unwrap();
    recorder.record(page("/icpplustiem/acCitar", 1, &["No hay citas disponibles", "Seleccione la oficina"]));
    let written = std::fs::read_to_string(run_dir(root.path()).join("0001.json")).unwrap();
    assert!(written.contains("No hay citas disponibles") && written.contains("Seleccione la oficina"));
}
#[test]
fn reads_are_due_at_most_every_second_and_a_half() {
    let root = tempfile::tempdir().unwrap();
    let mut recorder = LearningRecorder::create(root.path(), "run", &profile()).unwrap();
    let start = std::time::Instant::now();
    assert!(recorder.due(start));
    assert!(!recorder.due(start + std::time::Duration::from_millis(1400)));
    assert!(recorder.due(start + std::time::Duration::from_millis(1600)));
}
#[test]
fn two_runs_started_in_the_same_second_do_not_share_a_directory() {
    let root = tempfile::tempdir().unwrap();
    assert!(LearningRecorder::create(root.path(), "run", &profile()).is_some());
    assert!(LearningRecorder::create(root.path(), "run", &profile()).is_some());
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 2);
}

#[test]
fn a_failure_to_read_is_noted_once_per_kind_until_a_page_is_read_again() {
    let root = tempfile::tempdir().unwrap();
    let mut recorder = LearningRecorder::create(root.path(), "run", &profile()).unwrap();
    recorder.note("read_failed_call");
    recorder.note("read_failed_call");
    recorder.note("read_empty");
    recorder.record(page("/icpplustiem/a", 1, &[]));
    recorder.note("read_failed_call");
    let text = std::fs::read_to_string(run_dir(root.path()).join("events.jsonl")).unwrap();
    let events: Vec<Value> = text.lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    let names: Vec<_> = events.iter().map(|e| e["event"].as_str().unwrap()).collect();
    assert_eq!(names, ["read_failed_call", "read_empty", "read_failed_call"]);
}
