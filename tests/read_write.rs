use nfc_nci_sys::*;

use std::sync::mpsc;
use std::error::Error;
use libffi::high::Closure1;
use std::ffi::CString;

fn do_cleanup(res: Result<(), Box<dyn Error>>) -> Result<(), Box<dyn Error>> {
    unsafe { nfcManager_doDeinitialize() };
    res
}

#[test]
#[ignore]
fn it_initializes_nfc_manager() {
    // This test requires a card reader.
    unsafe {
        assert_eq!(nfcManager_doInitialize(), 0, 
            "Failed to initialize nfc manager. Note: this test requires a card reader to be attached");
        nfcManager_doDeinitialize(); 
    }
}

#[test]
#[ignore]
fn it_reads_and_writes_ndef_text() -> Result<(), Box<dyn Error>> {
    // This test requires a card reader and a compatible, writable tag.

    const TAG_READ_TIMEOUT_MS: u64 = 5000;
    let (tx, rx) = mpsc::channel();

    // This callback should not try to perform any nfc tag operations as it
    // seems to block the thread in which those operations are done. So send
    // the tag info back to the main thread and get out so the NFC management thread
    // can go about its business.
    let f = move | tag_info: *mut nfc_tag_info_t | {
        tx.send(tag_info).unwrap();
    };

    let closure = Closure1::new(&f);
    // let on_arrival = *closure.code_ptr();

    let mut tag_callback = 
        nfcTagCallback_t {
            onTagArrival: unsafe { Some(std::mem::transmute(*closure.code_ptr())) },
            onTagDeparture: None
        };
    
    unsafe {
        nfcManager_doInitialize();
        nfcManager_registerTagCallback(&mut tag_callback);
        nfcManager_enableDiscovery(DEFAULT_NFA_TECH_MASK, 0x01, 0, 0);
        assert_eq!(nfcManager_isNfcActive(), 1, "Failed to activate NFC manager. Is a NFC reader attached?");
    }

    // Wait for tag
    let tag_info = match rx.recv_timeout(std::time::Duration::from_millis(TAG_READ_TIMEOUT_MS)) {
        Ok(tag_info_ptr) => unsafe { *tag_info_ptr },
        Err(_) => return do_cleanup(Err("Timedout waiting for tag. Is there a tag near the reader?".into()))
    };
    let mut ndef_info: ndef_info_t = Default::default();

    unsafe {
        if nfcTag_isNdef(tag_info.handle, &mut ndef_info) != 1 {
            // Try to format the card
            nfcTag_formatTag(tag_info.handle);
            if nfcTag_isNdef(tag_info.handle, &mut ndef_info) != 1 {
                return do_cleanup(Err("Tag is not NDEF and could not be formatted.".into()))
            }
        }
    };

    // Allocate space in a vector for the NDEF.
    let mut ndef_content: Vec<std::os::raw::c_uchar> = Vec::with_capacity(100);

    const HELLO_RUST: &str = "Hello rust!";
    let language_code_ptr = CString::new("en").unwrap().into_raw();
    let text_content_ptr = CString::new(HELLO_RUST).unwrap().into_raw();
    let ndef_content_len;
    unsafe {
        ndef_content_len = ndef_createText(
            language_code_ptr, 
            text_content_ptr,
            ndef_content.as_mut_ptr(),
            ndef_content.capacity().try_into().unwrap()
        );
        // Make sure raw pointer memory is freed, per into_raw() docs
        let (_, _) = (CString::from_raw(language_code_ptr), CString::from_raw(text_content_ptr));
    };

    if ndef_content_len <= 0 { 
        return do_cleanup(Err("Failed to encode NDEF text.".into())) 
    } else {
        unsafe { ndef_content.set_len(ndef_content_len.try_into().unwrap()) };
    }
    
    unsafe {
        let res = nfcTag_writeNdef(tag_info.handle, ndef_content.as_mut_ptr(), ndef_content_len.try_into().unwrap());
        if  res != 0 {
            return do_cleanup(Err(format!("Failed to write to tag. Got {}", ndef_content_len).into()))
        }
    };

    let mut ndef_type: nfc_friendly_type_t = Default::default();
    let ndef_len = unsafe {
        // This writes into the content vector via raw pointer
        nfcTag_readNdef(tag_info.handle, ndef_content.as_mut_ptr(), ndef_info.current_ndef_length, &mut ndef_type)
    };

    if ndef_len == -1 || ndef_type != nfc_friendly_type_t_NDEF_FRIENDLY_TYPE_TEXT {
        return do_cleanup(Err("Failed to read NDEF text record from tag".into()))
    }
    // We have to tell the vector that we wrote in the space we allocated
    unsafe { ndef_content.set_len(ndef_len.try_into().unwrap()) };
    // The text content is less than the ndef content in size. Use u8 for conversion to CString.
    let mut text_content: Vec<u8> = Vec::with_capacity(ndef_len.try_into().unwrap());
    let text_len = unsafe {
        ndef_readText(
            ndef_content.as_mut_ptr(), ndef_len.try_into().unwrap(), 
            text_content.as_mut_ptr() as *mut std::os::raw::c_char, ndef_len.try_into().unwrap()
        )
    };
    if text_len != -1 {
        unsafe { text_content.set_len(text_len.try_into().unwrap()) };
        // let text = CString::new(&text_content[..text_len.try_into().unwrap()]).unwrap();
        let text = CString::new(text_content).unwrap();
        assert_eq!(text.to_str().unwrap(), HELLO_RUST);
    } else {
        return do_cleanup(Err("Failed to extract text from NDEF".into()))
    }

    do_cleanup(Ok(()))
}