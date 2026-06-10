/* i.MX RT1062 (Teensy 4.1) メモリマップ */
MEMORY
{
    /* 外付け QSPI Flash — コード・定数 (8 MB) */
    FLASH (rx)  : ORIGIN = 0x60000000, LENGTH = 8192K

    /* DTCM — スタック・静的変数 (512 KB) */
    RAM (rwx)   : ORIGIN = 0x20000000, LENGTH = 512K

    /* OCRAM — DMA バッファ等に使用可 (512 KB) */
    OCRAM (rwx) : ORIGIN = 0x20200000, LENGTH = 512K
}
