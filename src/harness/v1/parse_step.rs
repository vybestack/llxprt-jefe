//! Step-level schema-1 parsing (issue #380).
//!
//! Each step object carries an `op` discriminator; every variant is a closed
//! object. Interpolation grammar is validated here for env values and launch
//! argv (the only interpolable positions) so violations fail before launch.

use super::contract::{
    CaptureBehavior, CaptureExpectation, EnvVar, FileExpectation, Modifier, Step, WaitSource,
};
use super::error::HarnessError;
use super::fields::{ObjectReader, as_array, as_bool, as_int_in, as_str, bounded_len};
use super::interp;
use super::json::JsonValue;
use super::limits::{MAX_ARGV, MAX_BYTES, MAX_MODIFIERS, TIMEOUT_MS_RANGE};
use super::parse::{parse_content, parse_dir, parse_env_list, parse_file, parse_size};
use super::validate::{validate_id, validate_rel_path};

/// Parse one step object.
///
/// # Errors
///
/// `HAR-E001`/`HAR-E002`/`HAR-E003` per the contract.
pub fn parse_step(index: usize, value: &JsonValue) -> Result<Step, HarnessError> {
    let context = format!("steps[{index}]");
    let mut reader = ObjectReader::new(&context, value)?;
    let op = as_str(&format!("{context}.op"), reader.require("op")?)?.to_string();
    let step = match op.as_str() {
        "write" => parse_write(&context, &mut reader),
        "mkdir" => parse_mkdir(&context, &mut reader),
        "remove" => parse_remove(&context, &mut reader),
        "capture" => parse_capture(&context, &mut reader),
        "launch" => parse_launch(&context, &mut reader),
        "key" => parse_key(&context, &mut reader),
        "text" => parse_text(&context, &mut reader),
        "resize" => parse_resize(&context, &mut reader),
        "wait" => parse_wait(&context, &mut reader),
        "assert-frame" => parse_assert_frame(&context, &mut reader),
        "assert-capture" => parse_assert_capture(&context, &mut reader),
        "assert-file" => parse_assert_file(&context, &mut reader),
        "restart" => Ok(Step::Restart),
        "finish" => Ok(Step::Finish),
        other => Err(HarnessError::syntax(format!(
            "{context}.op: unknown op '{other}'"
        ))),
    }?;
    reader.finish()?;
    Ok(step)
}

fn parse_write(context: &str, reader: &mut ObjectReader<'_>) -> Result<Step, HarnessError> {
    let file = parse_file(&format!("{context}.file"), reader.require("file")?)?;
    Ok(Step::Write { file })
}

fn parse_mkdir(context: &str, reader: &mut ObjectReader<'_>) -> Result<Step, HarnessError> {
    let dir = parse_dir(&format!("{context}.dir"), reader.require("dir")?)?;
    Ok(Step::Mkdir { dir })
}

fn parse_remove(context: &str, reader: &mut ObjectReader<'_>) -> Result<Step, HarnessError> {
    let raw = as_str(&format!("{context}.path"), reader.require("path")?)?;
    let path = validate_rel_path(&format!("{context}.path"), raw)?;
    Ok(Step::Remove { path })
}

fn parse_capture(context: &str, reader: &mut ObjectReader<'_>) -> Result<Step, HarnessError> {
    let name = as_str(&format!("{context}.name"), reader.require("name")?)?;
    validate_id(&format!("{context}.name"), name)?;
    let raw_path = as_str(&format!("{context}.path"), reader.require("path")?)?;
    let path = validate_rel_path(&format!("{context}.path"), raw_path)?;
    let behavior = parse_behavior(&format!("{context}.behavior"), reader.require("behavior")?)?;
    Ok(Step::Capture {
        name: name.to_string(),
        path,
        behavior,
    })
}

fn parse_behavior(context: &str, value: &JsonValue) -> Result<CaptureBehavior, HarnessError> {
    let mut reader = ObjectReader::new(context, value)?;
    let stdout = as_str(&format!("{context}.stdout"), reader.require("stdout")?)?.to_string();
    let stderr = as_str(&format!("{context}.stderr"), reader.require("stderr")?)?.to_string();
    let exit_code = as_int_in(
        &format!("{context}.exit_code"),
        reader.require("exit_code")?,
        (0, 255),
    )?;
    let stdin_limit = as_int_in(
        &format!("{context}.stdin_limit"),
        reader.require("stdin_limit")?,
        (0, MAX_BYTES as u64),
    )?;
    let hang = as_bool(&format!("{context}.hang"), reader.require("hang")?)?;
    let spawn_child_hang = as_bool(
        &format!("{context}.spawn_child_hang"),
        reader.require("spawn_child_hang")?,
    )?;
    reader.finish()?;
    Ok(CaptureBehavior {
        stdout,
        stderr,
        exit_code: u8::try_from(exit_code)
            .map_err(|_| HarnessError::limit(format!("{context}.exit_code: out of range")))?,
        stdin_limit,
        hang,
        spawn_child_hang,
    })
}

fn parse_launch(context: &str, reader: &mut ObjectReader<'_>) -> Result<Step, HarnessError> {
    let argv_json = as_array(&format!("{context}.argv"), reader.require("argv")?)?;
    bounded_len(&format!("{context}.argv"), argv_json.len(), MAX_ARGV)?;
    let mut argv = Vec::with_capacity(argv_json.len());
    for (index, entry) in argv_json.iter().enumerate() {
        let entry_context = format!("{context}.argv[{index}]");
        let text = as_str(&entry_context, entry)?;
        interp::validate_value(&entry_context, text)?;
        argv.push(text.to_string());
    }
    let env = parse_env_list(&format!("{context}.env"), reader.require("env")?)?;
    for entry in &env {
        interp::validate_value(&format!("{context}.env.{}", entry.name), &entry.value)?;
    }
    let raw_cwd = as_str(&format!("{context}.cwd"), reader.require("cwd")?)?;
    let cwd = validate_rel_path(&format!("{context}.cwd"), raw_cwd)?;
    Ok(Step::Launch { argv, env, cwd })
}

fn parse_key(context: &str, reader: &mut ObjectReader<'_>) -> Result<Step, HarnessError> {
    let key = as_str(&format!("{context}.key"), reader.require("key")?)?;
    if key.is_empty() {
        return Err(HarnessError::syntax(format!(
            "{context}.key: must not be empty"
        )));
    }
    let modifiers_json = as_array(
        &format!("{context}.modifiers"),
        reader.require("modifiers")?,
    )?;
    bounded_len(
        &format!("{context}.modifiers"),
        modifiers_json.len(),
        MAX_MODIFIERS,
    )?;
    let mut modifiers = Vec::with_capacity(modifiers_json.len());
    for (index, entry) in modifiers_json.iter().enumerate() {
        let entry_context = format!("{context}.modifiers[{index}]");
        let modifier = match as_str(&entry_context, entry)? {
            "alt" => Modifier::Alt,
            "control" => Modifier::Control,
            "shift" => Modifier::Shift,
            other => {
                return Err(HarnessError::syntax(format!(
                    "{entry_context}: unknown modifier '{other}'"
                )));
            }
        };
        if modifiers.contains(&modifier) {
            return Err(HarnessError::syntax(format!(
                "{entry_context}: duplicate modifier"
            )));
        }
        modifiers.push(modifier);
    }
    Ok(Step::Key {
        key: key.to_string(),
        modifiers,
    })
}

fn parse_text(context: &str, reader: &mut ObjectReader<'_>) -> Result<Step, HarnessError> {
    let text = as_str(&format!("{context}.text"), reader.require("text")?)?;
    Ok(Step::Text {
        text: text.to_string(),
    })
}

fn parse_resize(context: &str, reader: &mut ObjectReader<'_>) -> Result<Step, HarnessError> {
    let size = parse_size(&format!("{context}.size"), reader.require("size")?)?;
    Ok(Step::Resize { size })
}

fn parse_wait(context: &str, reader: &mut ObjectReader<'_>) -> Result<Step, HarnessError> {
    let source = match as_str(&format!("{context}.source"), reader.require("source")?)? {
        "frame" => WaitSource::Frame,
        "stdout" => WaitSource::Stdout,
        "stderr" => WaitSource::Stderr,
        other => {
            return Err(HarnessError::syntax(format!(
                "{context}.source: unknown source '{other}'"
            )));
        }
    };
    let literal = as_str(&format!("{context}.literal"), reader.require("literal")?)?;
    if literal.is_empty() {
        return Err(HarnessError::syntax(format!(
            "{context}.literal: must not be empty"
        )));
    }
    let timeout_ms = as_int_in(
        &format!("{context}.timeout_ms"),
        reader.require("timeout_ms")?,
        TIMEOUT_MS_RANGE,
    )?;
    Ok(Step::Wait {
        source,
        literal: literal.to_string(),
        timeout_ms,
    })
}

fn parse_assert_frame(context: &str, reader: &mut ObjectReader<'_>) -> Result<Step, HarnessError> {
    let contains = parse_string_list(&format!("{context}.contains"), reader.require("contains")?)?;
    let absent = parse_string_list(&format!("{context}.absent"), reader.require("absent")?)?;
    Ok(Step::AssertFrame { contains, absent })
}

fn parse_assert_capture(
    context: &str,
    reader: &mut ObjectReader<'_>,
) -> Result<Step, HarnessError> {
    let capture = parse_expectation(&format!("{context}.capture"), reader.require("capture")?)?;
    Ok(Step::AssertCapture { capture })
}

fn parse_expectation(context: &str, value: &JsonValue) -> Result<CaptureExpectation, HarnessError> {
    let mut reader = ObjectReader::new(context, value)?;
    let name = as_str(&format!("{context}.name"), reader.require("name")?)?;
    validate_id(&format!("{context}.name"), name)?;
    let invocation = as_int_in(
        &format!("{context}.invocation"),
        reader.require("invocation")?,
        (1, u64::MAX),
    )?;
    let argv = parse_string_list(&format!("{context}.argv"), reader.require("argv")?)?;
    let env = parse_byte_pairs(&format!("{context}.env"), reader.require("env")?)?;
    let cwd = as_str(&format!("{context}.cwd"), reader.require("cwd")?)?.to_string();
    let stdin = opt_string(&mut reader, context, "stdin")?;
    let stdout = opt_string(&mut reader, context, "stdout")?;
    let stderr = opt_string(&mut reader, context, "stderr")?;
    let exit_code = match reader.opt("exit_code") {
        Some(value) => Some(
            u8::try_from(as_int_in(&format!("{context}.exit_code"), value, (0, 255))?)
                .map_err(|_| HarnessError::limit(format!("{context}.exit_code: out of range")))?,
        ),
        None => None,
    };
    let signal = match reader.opt("signal") {
        Some(JsonValue::Int(raw)) => Some(
            i32::try_from(*raw)
                .map_err(|_| HarnessError::limit(format!("{context}.signal: out of range")))?,
        ),
        Some(_) => {
            return Err(HarnessError::syntax(format!(
                "{context}.signal: expected an integer"
            )));
        }
        None => None,
    };
    reader.finish()?;
    Ok(CaptureExpectation {
        name: name.to_string(),
        invocation,
        argv,
        env,
        cwd,
        stdin,
        stdout,
        stderr,
        exit_code,
        signal,
    })
}

fn parse_assert_file(context: &str, reader: &mut ObjectReader<'_>) -> Result<Step, HarnessError> {
    let file_context = format!("{context}.file");
    let mut file_reader = ObjectReader::new(&file_context, reader.require("file")?)?;
    let raw_path = as_str(
        &format!("{file_context}.path"),
        file_reader.require("path")?,
    )?;
    let path = validate_rel_path(&format!("{file_context}.path"), raw_path)?;
    let exists = match file_reader.opt("exists") {
        Some(value) => as_bool(&format!("{file_context}.exists"), value)?,
        None => true,
    };
    let body = match file_reader.opt("content") {
        Some(value) => Some(parse_content(&format!("{file_context}.content"), value)?),
        None => None,
    };
    file_reader.finish()?;
    if !exists && body.is_some() {
        return Err(HarnessError::syntax(format!(
            "{file_context}: content cannot be asserted on a file expected to be absent"
        )));
    }
    Ok(Step::AssertFile {
        file: FileExpectation {
            path,
            exists,
            content: body,
        },
    })
}

fn parse_string_list(context: &str, value: &JsonValue) -> Result<Vec<String>, HarnessError> {
    let entries = as_array(context, value)?;
    let mut out = Vec::with_capacity(entries.len());
    for (index, entry) in entries.iter().enumerate() {
        out.push(as_str(&format!("{context}[{index}]"), entry)?.to_string());
    }
    Ok(out)
}

fn parse_byte_pairs(context: &str, value: &JsonValue) -> Result<Vec<EnvVar>, HarnessError> {
    let entries = as_array(context, value)?;
    let mut out = Vec::with_capacity(entries.len());
    for (index, entry) in entries.iter().enumerate() {
        let entry_context = format!("{context}[{index}]");
        let mut reader = ObjectReader::new(&entry_context, entry)?;
        let name = as_str(&format!("{entry_context}.name"), reader.require("name")?)?.to_string();
        let value_text =
            as_str(&format!("{entry_context}.value"), reader.require("value")?)?.to_string();
        reader.finish()?;
        out.push(EnvVar {
            name,
            value: value_text,
        });
    }
    Ok(out)
}

fn opt_string(
    reader: &mut ObjectReader<'_>,
    context: &str,
    field: &str,
) -> Result<Option<String>, HarnessError> {
    match reader.opt(field) {
        Some(value) => Ok(Some(
            as_str(&format!("{context}.{field}"), value)?.to_string(),
        )),
        None => Ok(None),
    }
}
