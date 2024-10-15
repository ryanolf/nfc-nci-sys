#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(deref_nullptr)]

/*!
Rust bindings for NXP's [linux_nfc-nci library](https://github.com/NXPNFCLinux/linux_libnfc-nci). Generated with bindgen.

linux_nfc-nci must be built and available for the target platform. Set the environment
variables NFC_NCI_LINUX_LIB_DIR and NFC_NCI_LINUX_INCLUDE_DIR to point to the relevant
directories continaing library and headers at build time. At runtime, the library must be
available in the library path, e.g. LD_LIBRARY_PATH on Linux.

For example, in the `vendor/linux_libnfc-nci` directory, run the following commands:

```sh
./bootstrap
./configure [--prefix=<prefix: default /usr/local>]
make
make install
```

Then, set the environment variables:

```sh
export NFC_NCI_LINUX_LIB_DIR=<prefix>/lib
export NFC_NCI_LINUX_INCLUDE_DIR=<prefix>/include
export LD_LIBRARY_PATH=$LD_LIBRARY_PATH:<prefix>/lib
```
Or set them in your `~/.cargo/config.toml` file whereever you're building from.

```toml
[env]
NFC_NCI_LINUX_INCLUDE_DIR = "<prefix>/include"
NFC_NCI_LINUX_LIB_DIR = "<prefix>/lib"
LD_LIBRARY_PATH = "<prefix>/lib"
```

Example code for writing and reading a tag using a connected, compatible reader.

```no_run
use nfc_nci_sys::*;
use std::error::Error;
use libffi::high::Closure1;
use std::ffi::CString;
use std::sync::mpsc;

fn do_cleanup(res: Result<(), Box<dyn Error>>) -> Result<(), Box<dyn Error>> {
    unsafe { nfcManager_doDeinitialize() };
    res
}

fn main() -> Result<(), Box<dyn Error>> {
    /// Write, formatting if necessary, and read a tag.

    const TAG_READ_TIMEOUT_MS: u64 = 5000;
    let (tx, rx) = mpsc::channel();

    // This callback should not try to perform any nfc tag operations as it
    // blocks the thread in which those operations are done. So send the tag
    // info back to the main thread and get out so the NFC management thread can
    // go about its business.
    let f = move | tag_info: *mut nfc_tag_info_t | {
        print!("Tag UID: ");
        unsafe {
            let uid_length = (*tag_info).uid_length;
            for ch in (*tag_info).uid[..uid_length.try_into().unwrap()].iter() {
                print!("{:02X}", ch);
            }
        }
        println!("");
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
        nfcManager_enableDiscovery(NFA_TECHNOLOGY_MASK_A, 0x01, 0, 0);
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
    // The text content is less than the ndef content in size.
    let mut text_content: Vec<u8> = Vec::with_capacity(ndef_len.try_into().unwrap());
    let text_len = unsafe {
        ndef_readText(
            ndef_content.as_mut_ptr(), ndef_len.try_into().unwrap(),
            text_content.as_mut_ptr() as *mut std::os::raw::c_char, ndef_len.try_into().unwrap()
        )
    };
    if text_len != -1 {
        unsafe { text_content.set_len(text_len.try_into().unwrap()) };
        let text = CString::new(&text_content[..text_len.try_into().unwrap()]).unwrap();
        assert_eq!(text.to_str().unwrap(), HELLO_RUST);
    } else {
        return do_cleanup(Err("Failed to extract text from NDEF".into()))
    }

    do_cleanup(Ok(()))
}

```
*/

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_encodes_and_decodes_NDEF() {
        // Use library to create and decode NDEF message to test round-trip.
        // Allocate space in a vector for the NDEF.

        use std::ffi::CString;

        const HELLO_RUST: &str = "Hello rust!";
        let mut ndef_content: Vec<std::os::raw::c_uchar> = Vec::with_capacity(100);
        let language_code_ptr = CString::new("en").unwrap().into_raw();
        let text_content_ptr = CString::new(HELLO_RUST).unwrap().into_raw();
        let ndef_content_len;
        unsafe {
            ndef_content_len = ndef_createText(
                language_code_ptr,
                text_content_ptr,
                ndef_content.as_mut_ptr(),
                ndef_content.capacity().try_into().unwrap(),
            );
            // Make sure raw pointer memory is freed, per into_raw() docs
            let (_, _) = (
                CString::from_raw(language_code_ptr),
                CString::from_raw(text_content_ptr),
            );
        };

        assert!(ndef_content_len > 0, "Failed to encode NDEF text.");

        unsafe { ndef_content.set_len(ndef_content_len.try_into().unwrap()) };
        // The text content is less than the ndef content in size.
        // let mut text_content: Vec<std::os::raw::c_char> = Vec::with_capacity(ndef_content.len());
        let mut text_content: Vec<u8> = Vec::with_capacity(ndef_content.len());

        let text_len = unsafe {
            ndef_readText(
                ndef_content.as_mut_ptr(),
                ndef_content_len.try_into().unwrap(),
                text_content.as_mut_ptr() as *mut std::os::raw::c_char,
                ndef_content_len.try_into().unwrap(),
            )
        };
        assert!(text_len != -1, "Failed to decode NDEF text.");

        unsafe { text_content.set_len(text_len.try_into().unwrap()) };
        let text = CString::new(&text_content[..text_len.try_into().unwrap()]).unwrap();
        assert_eq!(text.to_str().unwrap(), HELLO_RUST, "Wrong text!");
    }
}
