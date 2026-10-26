#import <Foundation/Foundation.h>
#import <IOKit/IOKitLib.h>
#import <IOUSBHost/IOUSBHost.h>

// The 2xRoller firmware does not start interrupt reports until the host sends
// HID SET_IDLE. macOS sends that request for keyboards and mice only.
//
// 0  SET_IDLE accepted
// 1  no registry entry for the HID service
// 2  no IOUSBHostDevice above that service
// 3  IOUSBHostDevice could not be opened
// 4  the device rejected SET_IDLE
int mh_usb_set_idle(uint64_t hid_registry_id) {
    @autoreleasepool {
        // MACH_PORT_NULL is the default IOKit port. kIOMainPortDefault is the
        // same port, and it is a linker symbol only on macOS 12 and newer.
        io_service_t entry = IOServiceGetMatchingService(
            MACH_PORT_NULL, IORegistryEntryIDMatching(hid_registry_id));
        if (entry == IO_OBJECT_NULL) {
            return 1;
        }
        for (int i = 0; i < 8 && !IOObjectConformsTo(entry, "IOUSBHostDevice"); i++) {
            io_registry_entry_t parent = IO_OBJECT_NULL;
            kern_return_t status = IORegistryEntryGetParentEntry(entry, kIOServicePlane, &parent);
            IOObjectRelease(entry);
            if (status != KERN_SUCCESS) {
                return 2;
            }
            entry = parent;
        }
        if (!IOObjectConformsTo(entry, "IOUSBHostDevice")) {
            IOObjectRelease(entry);
            return 2;
        }

        NSError *error = nil;
        // Options 0 opens the device without capture, so the HID driver keeps
        // the interface and hidapi can still read it.
        IOUSBHostDevice *device =
            [[IOUSBHostDevice alloc] initWithIOService:entry
                                               options:IOUSBHostObjectInitOptionsNone
                                                 queue:nil
                                                 error:&error
                                       interestHandler:nil];
        IOObjectRelease(entry);
        if (device == nil) {
            return 3;
        }

        // Interface recipient, SET_IDLE, duration 0, every report id.
        IOUSBDeviceRequest request = {
            .bmRequestType = 0x21,
            .bRequest = 0x0A,
            .wValue = 0,
            .wIndex = 0,
            .wLength = 0,
        };
        NSUInteger transferred = 0;
        BOOL accepted = [device sendDeviceRequest:request
                                              data:nil
                                  bytesTransferred:&transferred
                                 completionTimeout:2.0
                                             error:&error];
        [device destroy];
        return accepted ? 0 : 4;
    }
}
