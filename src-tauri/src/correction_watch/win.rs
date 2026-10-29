//! The focused text box through UI Automation, Windows' accessibility API.
//! Offsets count UTF-16 units, as on macOS.

use std::cell::RefCell;
use windows::core::Interface;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
};
use windows::Win32::System::Ole::SafeArrayDestroy;
use windows::Win32::System::Variant::VariantToBoolean;
use windows::Win32::UI::Accessibility::*;

thread_local! {
    static AUTOMATION: RefCell<Option<IUIAutomation>> = const { RefCell::new(None) };
}

/// This thread's UI Automation client, made on first use.
fn automation() -> Option<IUIAutomation> {
    AUTOMATION.with(|cell| {
        if cell.borrow().is_none() {
            unsafe {
                // A thread that already set up COM keeps its own mode.
                let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
                let client: IUIAutomation =
                    CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok()?;
                // A hung app must not stall the watcher.
                if let Ok(timeouts) = client.cast::<IUIAutomation2>() {
                    let _ = timeouts.SetConnectionTimeout(500);
                    let _ = timeouts.SetTransactionTimeout(500);
                }
                *cell.borrow_mut() = Some(client);
            }
        }
        cell.borrow().clone()
    })
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
pub fn focused_text() -> Option<(Vec<i32>, String)> {
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
