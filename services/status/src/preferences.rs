use crate::wifi;
use locales::t;
use num_traits::*;
use std::fmt::Display;
use userprefs::Manager;

pub trait PrefHandler {
    // If handle() returns true, it has handled the operation.
    fn handle(&mut self, op: usize) -> bool;

    fn claim_menumatic_menu(&mut self, cid: xous::CID);
}

#[derive(Debug, num_derive::FromPrimitive, num_derive::ToPrimitive, PartialEq, PartialOrd)]
enum DevicePrefsOp {
    RadioOnOnBoot,
    ConnectKnownNetworksOnBoot,
    AutobacklightOnBoot,
    AutobacklightTimeout,
    KeyboardLayout,
    WLANMenu,
    SetTime,
    SetTimezone,
    AudioOn,
    AudioOff,
    HeadsetVolume,
    EarpieceVolume,

    // Those are reserved for internal use
    UpdateMenuAudioEnabled = 399,
    UpdateMenuAudioDisabled,
}

#[derive(Debug, num_derive::FromPrimitive, num_derive::ToPrimitive, PartialEq, PartialOrd)]
pub enum PrefsMenuUpdateOp {
    // Those are reserved for internal use
    UpdateMenuAudioEnabled = 399,
    UpdateMenuAudioDisabled,
}

impl Display for DevicePrefsOp {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::AutobacklightOnBoot => write!(f, "{}", t!("prefs.autobacklight_enable", xous::LANG)),
            Self::AutobacklightTimeout => write!(f, "{}", t!("prefs.autobacklight_duration", xous::LANG)),
            Self::ConnectKnownNetworksOnBoot => write!(f, "Automatically connnect to networks"),
            Self::RadioOnOnBoot => write!(f, "WiFi kill switch"),
            Self::KeyboardLayout => write!(f, "Keyboard layout"),
            Self::WLANMenu => write!(f, "WiFi settings"),
            Self::SetTime => write!(f, "{}", t!("mainmenu.set_rtc", xous::LANG)),
            Self::SetTimezone => write!(f, "{}", t!("mainmenu.set_tz", xous::LANG)),
            Self::AudioOn => write!(f, "{}", "Enable audio subsystem"),
            Self::AudioOff => write!(f, "{}", "Disable audio subsystem"),
            Self::HeadsetVolume => write!(f, "{}", "Headset volume"),
            Self::EarpieceVolume => write!(f, "{}", "Speaker volume"),

            _ => unimplemented!("should not end up here!"),
        }
    }
}

#[derive(Debug)]
enum DevicePrefsError {
    PrefsError(userprefs::Error),
    XousError(xous::Error),
}

impl From<userprefs::Error> for DevicePrefsError {
    fn from(e: userprefs::Error) -> Self {
        Self::PrefsError(e)
    }
}

impl From<xous::Error> for DevicePrefsError {
    fn from(e: xous::Error) -> Self{
        Self::XousError(e)
    }
}

impl Display for DevicePrefsError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        use DevicePrefsError::*;

        match self {
            PrefsError(e) => write!(f, "Preferences engine error: {:?}", e),
            XousError(e) => write!(f, "Kernel error: {:#?}", e),
        }
    }
}

struct DevicePrefs {
    up: Manager,
    modals: modals::Modals,
    gam: gam::Gam,
    kbd: keyboard::Keyboard,
    time_ux_cid: xous::CID,
    codec: codec::Codec,
    menu: Option<gam::MenuMatic>,
    menu_manager_sid: xous::SID,
    menu_global_conn: xous::CID,
    status_cid: xous::CID,
}

impl PrefHandler for DevicePrefs {
    fn handle(&mut self, op: usize) -> bool {
        if match FromPrimitive::from_usize(op) {
            Some(other) => {
                let other: PrefsMenuUpdateOp = other;
                match other {
                    PrefsMenuUpdateOp::UpdateMenuAudioEnabled => self.alter_menu_audio_off(),
                    PrefsMenuUpdateOp::UpdateMenuAudioDisabled => self.alter_menu_audio_on(),
                }
                true
            }
            _ => {
                log::error!("Got unknown message");
                false
            }
        } {
            return true;
        }

        match FromPrimitive::from_usize(op) {
            Some(other) => {
                self.consume_menu_action(other);

                true
            }
            _ => {
                log::error!("Got unknown message");
                false
            }
        }
    }

    fn claim_menumatic_menu(&mut self, cid: xous::CID) {
        // TODO(gsora): we have to specify and handle a manager here, because of the audio on/off thing.
        let mut menus = self
            .actions()
            .iter()
            .map(|action| gam::MenuItem {
                name: xous_ipc::String::from_str(&action.to_string()),
                action_conn: Some(cid),
                action_opcode: action.to_u32().unwrap(),
                action_payload: gam::MenuPayload::Scalar([0, 0, 0, 0]),
                close_on_select: true,
            })
            .collect::<Vec<gam::MenuItem>>();

        menus.push(gam::MenuItem {
            name: xous_ipc::String::from_str(t!("mainmenu.closemenu", xous::LANG)),
            action_conn: None,
            action_opcode: 0,
            action_payload: gam::MenuPayload::Scalar([0, 0, 0, 0]),
            close_on_select: true,
        });

        self.menu = gam::menu_matic(menus, gam::PREFERENCES_MENU_NAME, Some(self.menu_manager_sid));
    }
}

impl DevicePrefs {
    fn new(xns: &xous_names::XousNames, time_ux_cid: xous::CID, menu_manager_sid: xous::SID, menu_conn: xous::CID, codec: codec::Codec, status_conn: xous::CID) -> Self {
        Self {
            up: Manager::new(),
            modals: modals::Modals::new(&xns).unwrap(),
            gam: gam::Gam::new(&xns).unwrap(),
            kbd: keyboard::Keyboard::new(&xns).unwrap(),
            time_ux_cid,
            codec,
            menu: None,
            menu_manager_sid,
            menu_global_conn: menu_conn,
            status_cid: status_conn,
        }
    }

    fn actions(&mut self) -> Vec<DevicePrefsOp> {
        use DevicePrefsOp::*;

        let mut ret = vec![
            RadioOnOnBoot,
            ConnectKnownNetworksOnBoot,
            AutobacklightOnBoot,
            AutobacklightTimeout,
            KeyboardLayout,
            WLANMenu,
            SetTime,
            SetTimezone,
        ];

        if self.codec.is_running().unwrap_or_default() {
            ret.push(AudioOff);

            // TODO(gsora): detect what volume to show
            match self.headphone_connected().unwrap() {
                true => ret.push(HeadsetVolume),
                false => ret.push(EarpieceVolume),
            }
        } else {
            ret.push(AudioOn)
        }

        ret
    }

    fn consume_menu_action(&mut self, action: DevicePrefsOp) {
        use DevicePrefsOp::*;

        let resp = match action {
            AutobacklightOnBoot => self.autobacklight_on_boot(),
            RadioOnOnBoot => self.radio_on_on_boot(),
            ConnectKnownNetworksOnBoot => self.connect_known_networks_on_boot(),
            AutobacklightTimeout => self.autobacklight_timeout(),
            KeyboardLayout => self.keyboard_layout(),
            WLANMenu => self.wlan_menu(),
            SetTime => self.set_time_menu(),
            SetTimezone => self.set_timezone_menu(),
            AudioOn => self.audio_on(),
            AudioOff => self.audio_off(),
            HeadsetVolume => self.headset_volume(),
            EarpieceVolume => self.earpiece_volume(),

            _ => unimplemented!("should not end up here!"),
        };

        resp.unwrap_or_else(|error| self.show_error_modal(error));
    }

    fn show_error_modal(&self, e: DevicePrefsError) {
        self.modals
            .show_notification(
                format!("{}: {}", t!("wlan.error", xous::LANG), e).as_str(),
                None,
            )
            .unwrap()
    }
}

impl DevicePrefs {
    fn autobacklight_on_boot(&mut self) -> Result<(), DevicePrefsError> {
        let cv = !self.up.autobacklight_on_boot_or_default()?; // note inversion of storage sense to make it on when false

        self.modals.add_list(vec![t!("prefs.yes", xous::LANG), t!("prefs.no", xous::LANG)]).unwrap();

        let new_result = !yes_no_to_bool( // inversion of storage sense: false = on
            self.modals
                .get_radiobutton(&format!("Current status: {}", bool_to_yes_no(cv)))
                .unwrap()
                .as_str(),
        );

        if cv { // already inverted, so the meaning is true (true is on)
            xous::send_message(self.status_cid, xous::Message::new_scalar(
                crate::StatusOpcode::EnableAutomaticBacklight.to_usize().unwrap(),
                0, 0, 0, 0)
            ).ok();
        } else {
            xous::send_message(self.status_cid, xous::Message::new_scalar(
                crate::StatusOpcode::DisableAutomaticBacklight.to_usize().unwrap(),
                0, 0, 0, 0)
            ).ok();
        }

        Ok(self.up.set_autobacklight_on_boot(new_result)?)
    }

    fn autobacklight_timeout(&self) -> Result<(), DevicePrefsError> {
        let cv = {
            let mut res = self.up.autobacklight_timeout_or_default()?;

            log::debug!("backlight timeout in store: {}", res);

            if res == 0 {
                res = 10;
            }

            res
        };

        log::debug!("backlight timeout in store after closure: {}", cv);

        let raw_timeout = self
            .modals
            .alert_builder(t!("prefs.autobacklight_duration_in_secs", xous::LANG))
            .field(
                Some(cv.to_string()),
                Some(|tf| match tf.as_str().parse::<u64>() {
                    Ok(_) => None,
                    Err(_) => Some(xous_ipc::String::from_str(
                        t!("prefs.autobacklight_err", xous::LANG),
                    )),
                }),
            )
            .build()
            .unwrap();

        let new_timeout = raw_timeout.first().as_str().parse::<u64>().unwrap(); // we know this is a number, we checked with validator;

        Ok(self.up.set_autobacklight_timeout(new_timeout)?)
    }

    fn radio_on_on_boot(&mut self) -> Result<(), DevicePrefsError> {
        let cv = self.up.radio_on_on_boot_or_default()?;

        self.modals.add_list(vec![t!("prefs.yes", xous::LANG), t!("prefs.no", xous::LANG)]).unwrap();

        let new_result = yes_no_to_bool(
            self.modals
                .get_radiobutton(&format!("Current status: {}", bool_to_yes_no(cv)))
                .unwrap()
                .as_str(),
        );

        Ok(self.up.set_radio_on_on_boot(new_result)?)
    }

    fn connect_known_networks_on_boot(&mut self) -> Result<(), DevicePrefsError> {
        let cv = self.up.connect_known_networks_on_boot_or_default()?;

        self.modals.add_list(vec![t!("prefs.yes", xous::LANG), t!("prefs.no", xous::LANG)]).unwrap();

        let new_result = yes_no_to_bool(
            self.modals
                .get_radiobutton(&format!("Current status: {}", bool_to_yes_no(cv)))
                .unwrap()
                .as_str(),
        );

        Ok(self.up.set_connect_known_networks_on_boot(new_result)?)
    }

    fn wlan_menu(&self) -> Result<(), DevicePrefsError> {
        std::thread::sleep(std::time::Duration::from_millis(100));
        self.gam.raise_menu(gam::WIFI_MENU_NAME).unwrap();

        Ok(())
    }

    fn set_time_menu(&self) -> Result<(), DevicePrefsError> {
        std::thread::sleep(std::time::Duration::from_millis(100));

        xous::send_message(
            self.time_ux_cid,
            xous::Message::new_scalar(
                crate::time::TimeUxOp::SetTime.to_usize().unwrap(),
                0,
                0,
                0,
                0,
            ),
        )
        .unwrap();

        Ok(())
    }

    fn set_timezone_menu(&self) -> Result<(), DevicePrefsError> {
        std::thread::sleep(std::time::Duration::from_millis(100));

        xous::send_message(
            self.time_ux_cid,
            xous::Message::new_scalar(
                crate::time::TimeUxOp::SetTimeZone.to_usize().unwrap(),
                0,
                0,
                0,
                0,
            ),
        )
        .unwrap();

        Ok(())
    }

    fn keyboard_layout(&mut self) -> Result<(), DevicePrefsError> {
        let kl = self.up.keyboard_layout_or_default()?;

        let mappings = vec!["QWERTY", "AZERTY", "QWERTZ", "Dvorak"];

        self.modals.add_list(mappings.clone()).unwrap();

        let new_result = self
            .modals
            .get_radiobutton(&format!("Current layout: {}", keyboard::KeyMap::from(kl)))
            .unwrap();

        let new_result = match mappings
            .iter()
            .position(|&elem| elem == new_result.as_str())
        {
            Some(val) => val,
            None => 0,
        };

        self.up.set_keyboard_layout(new_result)?;

        self.kbd
            .set_keymap(keyboard::KeyMap::from(new_result))
            .unwrap();

        Ok(())
    }

    fn audio_on(&mut self) -> Result<(), DevicePrefsError> {
        self.codec.setup_8k_stream()?;

        self.up.set_audio_enabled(true)?;
        self.alter_menu_audio_on();

        Ok(())
    }

    fn alter_menu_audio_on(&mut self) {
        let menu = self.menu.as_ref().unwrap();

        menu.delete_item(t!("mainmenu.closemenu", xous::LANG));
        menu.delete_item(&DevicePrefsOp::AudioOn.to_string());
        menu.add_item(gam::MenuItem {
            name: xous_ipc::String::from_str(&DevicePrefsOp::AudioOff.to_string()),
            action_conn: Some(self.menu_global_conn),
            action_opcode: DevicePrefsOp::AudioOff.to_u32().unwrap(),
            action_payload: gam::MenuPayload::Scalar([0, 0, 0, 0]),
            close_on_select: true,
        });
        menu.add_item(gam::MenuItem {
            name: xous_ipc::String::from_str(&DevicePrefsOp::EarpieceVolume.to_string()),
            action_conn: Some(self.menu_global_conn),
            action_opcode: DevicePrefsOp::EarpieceVolume.to_u32().unwrap(),
            action_payload: gam::MenuPayload::Scalar([0, 0, 0, 0]),
            close_on_select: true,
        });
        menu.add_item(gam::MenuItem {
            name: xous_ipc::String::from_str(&DevicePrefsOp::HeadsetVolume.to_string()),
            action_conn: Some(self.menu_global_conn),
            action_opcode: DevicePrefsOp::HeadsetVolume.to_u32().unwrap(),
            action_payload: gam::MenuPayload::Scalar([0, 0, 0, 0]),
            close_on_select: true,
        });

        menu.add_item(gam::MenuItem {
            name: xous_ipc::String::from_str(t!("mainmenu.closemenu", xous::LANG)),
            action_conn: None,
            action_opcode: 0,
            action_payload: gam::MenuPayload::Scalar([0, 0, 0, 0]),
            close_on_select: true,
        });
    }

    fn audio_off(&mut self) -> Result<(), DevicePrefsError> {
        self.codec.power_off()?;

        self.up.set_audio_enabled(false)?;
        self.alter_menu_audio_off();

        Ok(())
    }

    fn alter_menu_audio_off(&mut self) {
        let menu = self.menu.as_ref().unwrap();
        // hide volume toggles
        menu.delete_item(t!("mainmenu.closemenu", xous::LANG));
        menu.delete_item(&DevicePrefsOp::AudioOff.to_string());
        menu.delete_item(&DevicePrefsOp::EarpieceVolume.to_string());
        menu.delete_item(&DevicePrefsOp::HeadsetVolume.to_string());
        menu.add_item(gam::MenuItem {
            name: xous_ipc::String::from_str(&DevicePrefsOp::AudioOn.to_string()),
            action_conn: Some(self.menu_global_conn),
            action_opcode: DevicePrefsOp::AudioOn.to_u32().unwrap(),
            action_payload: gam::MenuPayload::Scalar([0, 0, 0, 0]),
            close_on_select: true,
        });
        menu.add_item(gam::MenuItem {
            name: xous_ipc::String::from_str(t!("mainmenu.closemenu", xous::LANG)),
            action_conn: None,
            action_opcode: 0,
            action_payload: gam::MenuPayload::Scalar([0, 0, 0, 0]),
            close_on_select: true,
        });
    }

    fn headphone_connected(&mut self) -> Result<bool, DevicePrefsError> {
        match self.codec.poll_headphone_state()? {
            codec::HeadphoneState::PresentWithMic => Ok(true),
            codec::HeadphoneState::PresentWithoutMic => Ok(true),
            _ => Ok(false),
        }
    }

    fn volume_slider(&mut self, title: &str, headset: bool) -> Result<(i32, u32), DevicePrefsError> {
        // We're aiming at 20 step levels in the UI, which is the result of dividing
        // 80dB levels available through codec by 4 (https://xkcd.com/221/).

        let current_level = match headset {
            true => self.up.headset_volume_or_default()?,
            false => self.up.earpiece_volume_or_default()?,
        };

        // we're going in the signed integer realm here, coerce val to i32
        let val = self.modals.slider(title, 0, 100, current_level, 5).unwrap() as i32;
        let db_val = percentage_to_db(val as u32);

        Ok((db_val as i32, val as u32))
    }

    fn headset_volume(&mut self) -> Result<(), DevicePrefsError> {
        let (db_val, slider_val) = self.volume_slider("Headset volume level", true)?;
        self.codec.set_headphone_volume(codec::VolumeOps::Set, Some(db_val as f32))?;
        self.up.set_headset_volume(slider_val)?;

        Ok(())
    }

    fn earpiece_volume(&mut self) -> Result<(), DevicePrefsError> {
        let (db_val, slider_val) = self.volume_slider("Earpiece volume level", false)?;
        self.codec.set_speaker_volume(codec::VolumeOps::Set, Some(db_val as f32))?;
        self.up.set_earpiece_volume(slider_val)?;
        Ok(())
    }
}

pub fn percentage_to_db(value: u32) -> i32 {
    let negated_val = 100 - value;

    (negated_val as i32 * -80)/100
}

fn yes_no_to_bool(val: &str) -> bool {
    if val == t!("prefs.yes", xous::LANG) {
        true
    } else if val == t!("prefs.no", xous::LANG) {
        false
    } else {
        unreachable!("cannot go here!");
    }
}

fn bool_to_yes_no(val: bool) -> String {
    match val {
        true => t!("prefs.yes", xous::LANG).to_owned(),
        false => t!("prefs.no", xous::LANG).to_owned(),
    }
}

pub fn start_background_thread(sid: xous::SID, status_cid: xous::CID) {
    let sid = sid.clone();
    std::thread::spawn(move || run_menu_thread(sid, status_cid));
}

fn run_menu_thread(sid: xous::SID, status_cid: xous::CID) {
    let xns = xous_names::XousNames::new().unwrap();

    let menu_conn = xous::connect(sid).unwrap();

    let menumatic_sid = xous::create_server().unwrap();

    // --------------------------- spawn a time UX manager thread
    let time_sid = xous::create_server().unwrap();
    let time_cid = xous::connect(time_sid).unwrap();
    crate::time::start_time_ux(time_sid);

    let codec = codec::Codec::new(&xns).unwrap();

    let mut handlers: Vec<Box<dyn PrefHandler>> = vec![
        Box::new(DevicePrefs::new(&xns, time_cid, menumatic_sid, menu_conn, codec, status_cid)),
        Box::new(wifi::WLANMan::new(&xns)),
    ];

    // claim menumatic's on all prefhandlers for this thread
    for handler in handlers.iter_mut() {
        handler.claim_menumatic_menu(menu_conn);
    }

    loop {
        let msg = xous::receive_message(sid).unwrap();
        log::debug!("Got message: {:?}", msg);

        let op = msg.body.id();

        for handler in handlers.iter_mut() {
            if handler.handle(op) {
                log::debug!("handler found!");
                break;
            }

            log::debug!("handler not found, iterating...");
        }
    }
}
