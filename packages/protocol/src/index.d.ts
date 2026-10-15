/** Local WebSocket API. See docs/protocol.md and docs/protocol/actions.md. */
export declare const PROTOCOL_VERSION = 1;
export declare const DEFAULT_WEBSOCKET_URL = "ws://127.0.0.1:9123";
export type ModuleModel = "M003K" | "M005S" | "M0064" | "M032LE" | "M4315";
export interface ModuleRef {
    model: ModuleModel;
    id: string;
}
/** LED index, or `"all"` on M003K and M005S. */
export type ControlTarget = number | "all";
export type LightingMode = "disable" | "static" | "breathing" | "strobe" | "spring" | "cycling" | "random" | "wave" | "water" | "rainbow";
export type KnobDirection = "cw" | "ccw";
export type RollerDirection = "up" | "down";
export interface KeyStatePayload {
    module: ModuleRef;
    control: number;
    pressed: boolean;
}
export interface KnobRotatedPayload {
    module: ModuleRef;
    control: number;
    direction: KnobDirection;
}
export interface KnobStatePayload {
    module: ModuleRef;
    control: number;
    pressed: boolean;
}
export interface SliderMovedPayload {
    module: ModuleRef;
    control: number;
    value: number;
}
export interface RollerRotatedPayload {
    module: ModuleRef;
    control: number;
    direction: RollerDirection;
}
export interface SetButtonImagePayload {
    module: ModuleRef;
    control: number;
    data_base64: string;
}
export interface SetScreenImagePayload {
    module: ModuleRef;
    data_base64: string;
}
export interface SetButtonColorPayload {
    module: ModuleRef;
    control: number;
    r: number;
    g: number;
    b: number;
}
export interface SetScreenColorPayload {
    module: ModuleRef;
    r: number;
    g: number;
    b: number;
}
export interface SetBacklightPayload {
    module: ModuleRef;
    brightness: number;
}
export interface SetLedModePayload {
    module: ModuleRef;
    control: ControlTarget;
    mode: LightingMode;
    save?: boolean;
}
export interface SetLedColorPayload {
    module: ModuleRef;
    control: ControlTarget;
    r: number;
    g: number;
    b: number;
    save?: boolean;
}
export interface SetLedBrightnessPayload {
    module: ModuleRef;
    control: ControlTarget;
    brightness: number;
}
export type InputAction = {
    action: "key_state";
    payload: KeyStatePayload;
} | {
    action: "knob_rotated";
    payload: KnobRotatedPayload;
} | {
    action: "knob_state";
    payload: KnobStatePayload;
} | {
    action: "slider_moved";
    payload: SliderMovedPayload;
} | {
    action: "roller_rotated";
    payload: RollerRotatedPayload;
};
export type OutputCommand = {
    action: "set_button_image";
    payload: SetButtonImagePayload;
} | {
    action: "set_screen_image";
    payload: SetScreenImagePayload;
} | {
    action: "set_button_color";
    payload: SetButtonColorPayload;
} | {
    action: "set_screen_color";
    payload: SetScreenColorPayload;
} | {
    action: "set_backlight";
    payload: SetBacklightPayload;
} | {
    action: "set_led_mode";
    payload: SetLedModePayload;
} | {
    action: "set_led_color";
    payload: SetLedColorPayload;
} | {
    action: "set_led_brightness";
    payload: SetLedBrightnessPayload;
};
export type ClientMessage = ({
    v: 1;
    type: "trigger";
    id?: string;
} & InputAction) | ({
    v: 1;
    type: "command";
    id?: string;
} & OutputCommand);
export interface HelloMessage {
    v: 1;
    type: "hello";
    daemon: "masterhubd";
    /** True when `modules` is not empty. */
    usb: boolean;
    /** Open modules, in daemon order. */
    modules: string[];
    /** Module name to pogo site, when the USB port is known. */
    sites: Record<string, string>;
}
export type EventMessage = ({
    v: 1;
    type: "event";
} & InputAction) | {
    v: 1;
    type: "event";
    action: "set_button_image";
    payload: SetButtonImagePayload;
} | {
    v: 1;
    type: "event";
    action: "modules";
    payload: {
        modules: string[];
        sites: Record<string, string>;
    };
};
export interface ResultMessage {
    v: 1;
    type: "result";
    id?: string;
    /** Accepted by the daemon. This does not by itself mean the hardware changed. */
    ok: true;
}
export interface ErrorMessage {
    v: 1;
    type: "error";
    id?: string;
    message: string;
}
export type DaemonMessage = HelloMessage | EventMessage | ResultMessage | ErrorMessage;
