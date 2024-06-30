## 2章

### 2.1 開発環境
WSL2でのSDL2の利用で少し詰まった
```
# SDL2のインストール
brew install sdl2 sdl2_gfx sdl2_image sdl2_ttf

# DISPLAYの設定
export DISPLAY=:0
```

Xserver時代のDISPLAYの設定が残ってたせいだと思ったけど、
zshrcからexport消してもlocalhostのIPが勝手に設定されてるみたいだったので謎

参考
https://qiita.com/momomo_rimoto/items/1f378d475e3262ee605d

https://www.xmisao.com/2021/01/11/setup-cross-platform-rust-sdl2-project.html
からmain.rs借りて実行してGUIが表示できることを確認できたので一旦ヨシ

