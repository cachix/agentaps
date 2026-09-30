use agentaps_control_protocol::{Agent, AgentOption, Command, Project};
use async_channel::{Receiver, Sender};
use gpui_kit::component::{
    ActiveTheme, Root,
    input::{Input, InputEvent, InputState, Textarea, TextareaState},
    text::{TextView, TextViewStyle},
};
use gpui_kit::{
    App, ApplicationHandle, Context, Entity, IntoElement, Render, ScrollHandle,
    StatefulInteractiveElement, Window, WindowOptions, div, prelude::*, px, relative, rems, rgb,
};
// Embed the default component icons so Web Connect does not depend on runtime icon URLs.
gpui_kit::assets::icon_assets!(
    WebAssets,
    [
        ALargeSmall,
        ArrowDown,
        ArrowLeft,
        ArrowRight,
        ArrowUp,
        Asterisk,
        Ban,
        BatteryCharging,
        BatteryFull,
        BatteryLow,
        BatteryMedium,
        BatteryWarning,
        Battery,
        Bell,
        BookOpen,
        Bot,
        Building2,
        Calendar,
        CaseSensitive,
        ChartPie,
        Check,
        ChevronDown,
        ChevronLeft,
        ChevronRight,
        ChevronUp,
        ChevronsUpDown,
        CircleAlert,
        CircleCheck,
        CircleUser,
        CircleX,
        Close,
        Copy,
        Cpu,
        Dash,
        Delete,
        EllipsisVertical,
        Ellipsis,
        ExternalLink,
        EyeOff,
        Eye,
        FileText,
        File,
        FolderClosed,
        FolderOpen,
        Folder,
        Frame,
        GalleryVerticalEnd,
        Github,
        Globe,
        HardDrive,
        HeartOff,
        Heart,
        Inbox,
        Info,
        Inspector,
        LayoutDashboard,
        LoaderCircle,
        Loader,
        Map,
        Maximize,
        MemoryStick,
        Menu,
        Minimize,
        Minus,
        Moon,
        Network,
        Palette,
        PanelBottomOpen,
        PanelBottom,
        PanelLeftClose,
        PanelLeftOpen,
        PanelLeft,
        PanelRightClose,
        PanelRightOpen,
        PanelRight,
        Pause,
        Play,
        Plus,
        Redo2,
        Redo,
        RefreshCw,
        Replace,
        ResizeCorner,
        RotateCw,
        Search,
        Settings2,
        Settings,
        SortAscending,
        SortDescending,
        SquareTerminal,
        StarFill,
        StarOff,
        Star,
        Sun,
        ThumbsDown,
        ThumbsUp,
        TriangleAlert,
        Undo2,
        Undo,
        User,
        WindowClose,
        WindowMaximize,
        WindowMinimize,
        WindowRestore,
    ]
);

use iroh::EndpointId;

use std::{borrow::Cow, cell::RefCell};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
mod browser;
mod connection;
mod ui;
use browser::*;
use connection::exchange_pairing;
use ui::MobileView;

const BG: u32 = 0x10151c;
const SURFACE: u32 = 0x202a36;
const TEXT: u32 = 0xedf2f7;
const MUTED: u32 = 0x9aaaba;
const ACCENT: u32 = 0x8fc5ec;

thread_local! {
    static APPLICATION: RefCell<Option<ApplicationHandle>> = const { RefCell::new(None) };
}

#[wasm_bindgen(start)]
pub fn start() {
    gpui_kit::platform::web_init();
    let app = gpui_kit::platform::single_threaded_web().with_assets(WebAssets);
    APPLICATION.with(|application| {
        *application.borrow_mut() = Some(app.run_embedded(|cx: &mut App| {
            gpui_kit::init(cx);
            cx.text_system()
                .add_fonts(vec![Cow::Borrowed(include_bytes!(
                    "../fonts/IBMPlexSans-Regular.ttf"
                ))])
                .expect("Could not load font");
            cx.open_window(WindowOptions::default(), |window, cx| {
                let remote = pairing();
                let auto_scan = remote.is_none() && saved_connections().is_empty();
                let view = cx.new(|cx| MobileView::new(window, cx, remote));
                let root = cx.new(|cx| Root::new(view.clone(), window, cx));
                if auto_scan {
                    view.update(cx, |view, cx| view.scan_qr(window, cx));
                }
                root
            })
            .expect("Could not open browser window");
            cx.activate(true);
        }));
    });
}
