//! The focused text box through UI Automation, Windows' accessibility API.
//! Offsets count UTF-16 units, as on macOS. Each query runs on a thread of
//! its own: called from the UI thread (the paste runs there), UI Automation
//! can hang the app.

use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
};
use windows::Win32::System::Ole::SafeArrayDestroy;
use windows::Win32::System::Variant::VariantToBoolean;
use windows::Win32::UI::Accessibility::*;

/// Runs `query` on a new thread with its own COM set-up, and waits for it.
fn on_own_thread<T: Send>(query: impl FnOnce() -> Option<T> + Send) -> Option<T> {
    std::thread::scope(|s| {
        s.spawn(|| {
            if unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_err() {
                return None;
            }
            // Dropped after the query's objects, even if it panics.
            let _com = Com;
            query()
        })
        .join()
        .ok()
        .flatten()
    })
}

/// Undoes the query thread's COM set-up.
struct Com;

impl Drop for Com {
    fn drop(&mut self) {
        unsafe { CoUninitialize() }
    }
}

/// A UI Automation client for this query's thread. The Windows 8 client,
/// since only it takes timeouts: a hung app must not stall the paste.
fn automation() -> Option<IUIAutomation2> {
    unsafe {
        let client: IUIAutomation2 =
            CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER).ok()?;
        client.SetConnectionTimeout(500).ok()?;
        client.SetTransactionTimeout(500).ok()?;
        Some(client)
    }
}

/// The focused element, unless it's a password field.
fn focused_field() -> Option<IUIAutomationElement> {
    unsafe {
        let focused = automation()?.GetFocusedElement().ok()?;
        if focused.CurrentIsPassword().ok()?.as_bool() {
            return None;
        }
        Some(focused)
    }
}

/// The element's runtime id, which stays the same while it exists.
fn runtime_id(element: &IUIAutomationElement) -> Option<Vec<i32>> {
    unsafe {
        let array = element.GetRuntimeId().ok()?;
        if array.is_null() {
            return None;
        }
        let ids = std::slice::from_raw_parts(
            (*array).pvData as *const i32,
            (*array).rgsabound[0].cElements as usize,
        )
        .to_vec();
        let _ = SafeArrayDestroy(array);
        Some(ids)
    }
}

/// The focused text box (by runtime id) and its text, unless it's a
/// password field or doesn't expose its text.
/// Line breaks come back as \n, as Owl pastes them (edit boxes give \r\n,
/// rich editors \r).
pub fn focused_text() -> Option<(Vec<i32>, String)> {
    on_own_thread(focused_text_here)
}

fn focused_text_here() -> Option<(Vec<i32>, String)> {
    let focused = focused_field()?;
    let text = unsafe {
        let value = focused
            .GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
            .and_then(|v| v.CurrentValue())
            .ok()
            .map(|v| v.to_string());
        // Rich editors (Word, Notepad) give their text only as a document.
        match value {
            Some(v) if !v.is_empty() => Some(v),
            value => focused
                .GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId)
                .and_then(|t| t.DocumentRange()?.GetText(-1))
                .ok()
                .map(|t| t.to_string())
                .or(value),
        }
    }?;
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    Some((runtime_id(&focused)?, text))
}

/// Whether two reads came from the same text box.
pub fn same(a: &[i32], b: &[i32]) -> bool {
    a == b
}

/// The text box's text pattern and its selection (or cursor, when empty).
fn selection(
    field: &IUIAutomationElement,
) -> Option<(IUIAutomationTextPattern, IUIAutomationTextRange)> {
    unsafe {
        let text = field
            .GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId)
            .ok()?;
        let ranges = text.GetSelection().ok()?;
        if ranges.Length().ok()? < 1 {
            return None;
        }
        let selected = ranges.GetElement(0).ok()?;
        Some((text, selected))
    }
}

pub fn selection_spot() -> Option<(u64, (isize, isize), String)> {
    on_own_thread(selection_spot_here)
}

fn selection_spot_here() -> Option<(u64, (isize, isize), String)> {
    use std::hash::{Hash, Hasher};
    let field = focused_field()?;
    let (text, selected) = selection(&field)?;
    unsafe {
        let selected_text = selected.GetText(-1).ok()?;
        // UI Automation has no offsets: the start is the length of the text
        // before the selection.
        let before = text.DocumentRange().ok()?;
        before
            .MoveEndpointByRange(
                TextPatternRangeEndpoint_End,
                &selected,
                TextPatternRangeEndpoint_Start,
            )
            .ok()?;
        let start = before.GetText(-1).ok()?.len() as isize;
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        runtime_id(&field)?.hash(&mut hasher);
        Some((
            hasher.finish(),
            (start, selected_text.len() as isize),
            selected_text.to_string(),
        ))
    }
}

pub fn no_text_box() -> bool {
    on_own_thread(|| Some(no_text_box_here())).unwrap_or(false)
}

fn no_text_box_here() -> bool {
    // Controls that never take typing. Anything else (a pane, a window, no
    // focus at all) may be an app hiding its text box, so it's pasted to.
    const NOT_TEXT: [UIA_CONTROLTYPE_ID; 20] = [
        UIA_DocumentControlTypeId,
        UIA_HyperlinkControlTypeId,
        UIA_ButtonControlTypeId,
        UIA_ImageControlTypeId,
        UIA_ListControlTypeId,
        UIA_ListItemControlTypeId,
        UIA_TreeControlTypeId,
        UIA_TreeItemControlTypeId,
        UIA_TableControlTypeId,
        UIA_DataGridControlTypeId,
        UIA_DataItemControlTypeId,
        UIA_CheckBoxControlTypeId,
        UIA_RadioButtonControlTypeId,
        UIA_SplitButtonControlTypeId,
        UIA_MenuItemControlTypeId,
        UIA_TabControlTypeId,
        UIA_TabItemControlTypeId,
        UIA_SliderControlTypeId,
        UIA_TextControlTypeId,
        UIA_HeaderItemControlTypeId,
    ];
    let Some(field) = focused_field() else {
        return false;
    };
    unsafe {
        if !field
            .CurrentControlType()
            .is_ok_and(|kind| NOT_TEXT.contains(&kind))
        {
            return false;
        }
        // A document you can type into (Word, Notepad, a contenteditable
        // page) is still a text box.
        if let Ok(value) =
            field.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
        {
            if value.CurrentIsReadOnly().is_ok_and(|r| !r.as_bool()) {
                return false;
            }
        }
        let editable = field
            .GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId)
            .and_then(|t| {
                t.DocumentRange()?
                    .GetAttributeValue(UIA_IsReadOnlyAttributeId)
            })
            .is_ok_and(|read_only| VariantToBoolean(&read_only).is_ok_and(|r| !r.as_bool()));
        !editable
    }
}

pub fn char_before_cursor() -> Option<char> {
    on_own_thread(char_before_cursor_here)
}

fn char_before_cursor_here() -> Option<char> {
    let field = focused_field()?;
    let (_, selected) = selection(&field)?;
    unsafe {
        // The selection's start, widened back by one character.
        let before = selected.Clone().ok()?;
        before
            .MoveEndpointByRange(
                TextPatternRangeEndpoint_End,
                &selected,
                TextPatternRangeEndpoint_Start,
            )
            .ok()?;
        before
            .MoveEndpointByUnit(TextPatternRangeEndpoint_Start, TextUnit_Character, -1)
            .ok()?;
        before.GetText(-1).ok()?.to_string().chars().last()
    }
}
