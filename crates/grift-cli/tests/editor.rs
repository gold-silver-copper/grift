//! The editor example's own code, given keys and files by a stand-in host.

use grift::{Error, Grift, Value};
use std::collections::VecDeque;

const EDITOR: &str = include_str!("../../../examples/emacs/src/editor.grift");

/// The keys left to type and each text the editor saved.
#[derive(Default)]
struct Host {
    keys: VecDeque<u8>,
    saved: Vec<String>,
}

type G = Grift<Host>;

fn key(g: &mut G, h: &mut Host, _: &[Value]) -> Result<Value, Error> {
    match h.keys.pop_front() {
        Some(b) => Ok(Value::Integer(i64::from(b))),
        None => Err(g.error("end-of-input", &[])),
    }
}

fn terminal_write(_: &mut G, _: &mut Host, _: &[Value]) -> Result<Value, Error> {
    Ok(Value::Inert)
}

fn terminal_size(g: &mut G, _: &mut Host, _: &[Value]) -> Result<Value, Error> {
    let columns = g.pair(Value::Integer(80), Value::EmptyList)?;
    g.pair(Value::Integer(24), columns)
}

fn read_file(g: &mut G, _: &mut Host, _: &[Value]) -> Result<Value, Error> {
    g.string("precious\n")
}

fn write_file(g: &mut G, h: &mut Host, args: &[Value]) -> Result<Value, Error> {
    h.saved.extend(g.as_string(&args[1])?);
    Ok(Value::Inert)
}

/// What the editor saved, with `notes` open, after `keys`.
fn saved_after(keys: &[u8]) -> Vec<String> {
    let mut h = Host {
        keys: keys.iter().copied().collect(),
        saved: Vec::new(),
    };
    let mut g: G = Grift::new().unwrap();
    let hosts: [(&str, grift::HostFunction<Host>); 5] = [
        ("key", key),
        ("terminal-write", terminal_write),
        ("terminal-size", terminal_size),
        ("read-file", read_file),
        ("write-file", write_file),
    ];
    for (name, f) in hosts {
        let f = g.host_function(name, f).unwrap();
        g.define(name, f).unwrap();
    }
    g.evaluate(&mut h, EDITOR).unwrap();
    // The run ends with an error when the keys do, or at a command the last keys named.
    let _ = g.evaluate(&mut h, "(editor-main \"notes\" \"\")");
    assert!(h.keys.is_empty(), "the keys left: {:?}", h.keys);
    h.saved
}

#[test]
fn saving_from_a_minibuffer_leaves_the_file_alone() {
    // C-x C-s saves the file's text; the same keys at an M-x prompt save nothing.
    assert_eq!(saved_after(b"\x18\x13"), ["precious\n"]);
    assert_eq!(saved_after(b"\x1bx\x18\x13notes\r"), [""; 0]);
}
