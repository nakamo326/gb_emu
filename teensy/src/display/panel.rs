pub struct InitCmd {
    pub cmd: u8,
    pub data: &'static [u8],
    pub delay_ms: u32,
}

pub trait PanelController {
    const WIDTH: u16;
    const HEIGHT: u16;
    const COL_OFFSET: u16;
    const ROW_OFFSET: u16;
    fn init_sequence() -> &'static [InitCmd];
}

const SWRESET: u8 = 0x01;
const SLPOUT: u8 = 0x11;
const COLMOD: u8 = 0x3A;
const MADCTL: u8 = 0x36;
const DISPON: u8 = 0x29;
const PWCTRL1: u8 = 0xC0;
const PWCTRL2: u8 = 0xC1;
const VMCTRL1: u8 = 0xC5;
const VMCTRL2: u8 = 0xC7;
const FRMCTR1: u8 = 0xB1;
const DFUNCTR: u8 = 0xB6;
const GAMSET: u8 = 0x26;
const PVGAMCTRL: u8 = 0xE0;
const NVGAMCTRL: u8 = 0xE1;
const INVON: u8 = 0x21;

pub struct Ili9341;

impl PanelController for Ili9341 {
    const WIDTH: u16 = 240;
    const HEIGHT: u16 = 320;
    const COL_OFFSET: u16 = 0;
    const ROW_OFFSET: u16 = 0;

    fn init_sequence() -> &'static [InitCmd] {
        static SEQ: &[InitCmd] = &[
            InitCmd {
                cmd: SWRESET,
                data: &[],
                delay_ms: 120,
            },
            InitCmd {
                cmd: SLPOUT,
                data: &[],
                delay_ms: 150,
            },
            InitCmd {
                cmd: PWCTRL1,
                data: &[0x23],
                delay_ms: 0,
            },
            InitCmd {
                cmd: PWCTRL2,
                data: &[0x10],
                delay_ms: 0,
            },
            InitCmd {
                cmd: VMCTRL1,
                data: &[0x3E, 0x28],
                delay_ms: 0,
            },
            InitCmd {
                cmd: VMCTRL2,
                data: &[0x86],
                delay_ms: 0,
            },
            InitCmd {
                cmd: MADCTL,
                data: &[0x48],
                delay_ms: 0,
            },
            InitCmd {
                cmd: COLMOD,
                data: &[0x55],
                delay_ms: 0,
            },
            InitCmd {
                cmd: FRMCTR1,
                data: &[0x00, 0x18],
                delay_ms: 0,
            },
            InitCmd {
                cmd: DFUNCTR,
                data: &[0x08, 0x82, 0x27],
                delay_ms: 0,
            },
            InitCmd {
                cmd: GAMSET,
                data: &[0x01],
                delay_ms: 0,
            },
            InitCmd {
                cmd: PVGAMCTRL,
                data: &[
                    0x0F, 0x31, 0x2B, 0x0C, 0x0E, 0x08, 0x4E, 0xF1, 0x37, 0x07, 0x10, 0x03, 0x0E,
                    0x09, 0x00,
                ],
                delay_ms: 0,
            },
            InitCmd {
                cmd: NVGAMCTRL,
                data: &[
                    0x00, 0x0E, 0x14, 0x03, 0x11, 0x07, 0x31, 0xC1, 0x48, 0x08, 0x0F, 0x0C, 0x31,
                    0x36, 0x0F,
                ],
                delay_ms: 0,
            },
            InitCmd {
                cmd: DISPON,
                data: &[],
                delay_ms: 10,
            },
        ];
        SEQ
    }
}

pub struct St7789;

impl PanelController for St7789 {
    const WIDTH: u16 = 240;
    const HEIGHT: u16 = 320;
    const COL_OFFSET: u16 = 0;
    const ROW_OFFSET: u16 = 0;

    fn init_sequence() -> &'static [InitCmd] {
        static SEQ: &[InitCmd] = &[
            InitCmd {
                cmd: SWRESET,
                data: &[],
                delay_ms: 150,
            },
            InitCmd {
                cmd: SLPOUT,
                data: &[],
                delay_ms: 150,
            },
            InitCmd {
                cmd: COLMOD,
                data: &[0x55],
                delay_ms: 0,
            },
            InitCmd {
                cmd: MADCTL,
                data: &[0x00],
                delay_ms: 0,
            },
            InitCmd {
                cmd: INVON,
                data: &[],
                delay_ms: 0,
            },
            InitCmd {
                cmd: DISPON,
                data: &[],
                delay_ms: 10,
            },
        ];
        SEQ
    }
}
