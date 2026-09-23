# Rust 창·렌더링 스택 조사 (issue #2)

조사일: 2026-09-23 · 대상 버전: winit 0.30.13 (0.31.0-beta.3 존재), wgpu 30.0.1, softbuffer 0.4.8, tiny-skia 0.12.0, resvg 0.48.1, image 0.25.10, tao 0.37.0 / Tauri 2.11.x, egui/eframe 0.36.2, Slint 1.18.1, iced 0.14.0, tray-icon 0.25.1 (버전은 crates.io API 기준)

표기: **[검증]** = 1차 자료(크레이트 소스/공식 문서/이슈)에서 직접 확인. **[추론]** = 자료를 근거로 한 판단이며 프로토타입으로 확인이 필요.

## 1. 결론 요약

**권장 스택: `winit 0.30` + CPU 렌더링(`tiny-skia` / `resvg` / `image`) + 플랫폼별 얇은 프레젠터·히트테스트 모듈 + `tray-icon`(Linux는 `ksni` 백엔드).**

- 어떤 후보도 "픽셀 단위 click-through"를 크로스플랫폼 API로 제공하지 않는다. winit / tao / egui / iced / Slint 모두 **창 전체 on/off 토글**만 있다. 따라서 어느 스택을 고르든 "커서가 불투명 픽셀(또는 히트박스) 위에 있으면 hittest on, 아니면 off" 토글 루프, 또는 플랫폼 네이티브 입력 영역 설정을 직접 구현해야 한다.
- 이 경우 GUI 툴킷(egui/iced/Slint)이나 웹뷰(Tauri)는 얻는 것 없이 무게와 투명도 버그만 늘린다. 스프라이트/GIF/APNG/SVG 한 장을 그리는 펫에는 winit + CPU 래스터가 가장 단순하다.
- **softbuffer 0.4.8(최신 릴리스)은 투명도를 지원하지 않는다**(픽셀 포맷 `00000000RRRRRRRRGGGGGGGGBBBBBBBB`). 투명도 PR은 main에 머지됐으나 미릴리스이고, 그 PR에서도 Win32/X11 투명은 미구현이다. 그래서 프레젠트 단계는 플랫폼별로 직접 작성하거나 wgpu를 써야 한다.
- 플랫폼별 핵심 주의점:
  - **Windows**: `WS_EX_LAYERED` + `UpdateLayeredWindow`(프리멀티플라이드 BGRA DIB)로 그리면 **OS가 알파 0 픽셀을 자동으로 click-through** 해 준다(유일한 "네이티브 픽셀 단위" 경로). `WM_NCHITTEST`의 `HTTRANSPARENT`는 같은 스레드의 창에만 전달되므로 다른 앱으로 클릭을 넘기는 데 쓸 수 없다. wgpu를 쓸 경우 DX12 `DxgiFromVisual`(DirectComposition)이어야 투명이 된다.
  - **macOS**: `setIgnoresMouseEvents`는 창 전체 단위. 무시 상태에서는 마우스 이벤트가 오지 않으므로 `NSEvent.mouseLocation` 폴링으로 토글해야 한다.
  - **Linux X11**: XShape 입력 영역에 사각형 목록/1비트 비트맵을 넣어 **네이티브 픽셀(또는 사각형) 단위 입력 영역** 가능. winit은 빈 영역/전체 영역만 설정하므로 x11rb로 직접 호출.
  - **Wayland**: winit에서 always-on-top(`set_window_level`)은 no-op, `outer_position`/`set_outer_position` 미지원, 전역 커서 좌표 없음. → **XWayland 강제(`EventLoopBuilderExtX11::with_x11()`)** 가 현실적인 best-effort.
- 대안 설계(참고 구현 clawd-on-desk 방식): 렌더 창은 항상 click-through, 히트박스 크기의 **별도 "hit 창"** 을 따라다니게 하는 2창 구조. 픽셀 폴링이 필요 없고 X11/Windows/macOS 모두 동일 로직. 픽셀 정확도 대신 히트박스 정확도.

## 2. 요구사항별 1차 자료

### 2.1 투명 창

- winit `WindowAttributes::with_transparent` / `Window::set_transparent`: "just a hint"; **X11은 생성 시에만** 설정 가능; macOS는 배경색을 리셋. [검증] winit 0.30.13 `src/window.rs` (https://docs.rs/winit/0.30.13/winit/window/struct.Window.html#method.set_transparent)
- 창이 투명해도 **실제 픽셀 알파를 컴포지터에 넘기는 것은 렌더러 몫**이다.
  - softbuffer 0.4.8: 픽셀 포맷 상위 8비트가 0으로 고정, 알파 없음. [검증] softbuffer 0.4.8 `src/lib.rs` 문서 주석 (https://docs.rs/softbuffer/0.4.8/softbuffer/struct.Buffer.html)
  - softbuffer 투명도 PR #321(`AlphaMode`) 머지로 issue #17 종료(2026-03-05). 그러나 최신 릴리스는 0.4.8(2025-12-13)이고, PR 본문에 "Win32: ... only did part of it", "X11: Transparency not yet implemented". [검증] https://github.com/rust-windowing/softbuffer/pull/321 , https://github.com/rust-windowing/softbuffer/issues/17
  - wgpu: `CompositeAlphaMode`로 설정하지만 지원 모드는 드라이버/백엔드 의존. Windows DX12 기본(`DxgiFromHwnd`)은 "does not support transparency", `DxgiFromVisual`(DirectComposition)은 "supports transparent windows". [검증] wgpu-types 30.0.0 `src/backend.rs` `Dx12SwapchainKind` (https://docs.rs/wgpu-types/latest/wgpu_types/enum.Dx12SwapchainKind.html)
  - AMD 최신 드라이버의 Vulkan 스왑체인은 DXGI 위에 구현되어 Opaque만 노출 → 검은 배경. 메인테이너가 dx12 + DirectComposition 옵션을 우회책으로 제시, Vulkan용은 #8354 진행 중. [검증] https://github.com/gfx-rs/wgpu/issues/5368 , https://github.com/gfx-rs/wgpu/issues/5150
  - tiny-skia `Pixmap`은 "premultiplied RGBA pixels"를 보관하는 CPU 전용 2D 렌더러, "focus on rendering quality, speed and binary size". [검증] tiny-skia 0.12.0 `src/pixmap.rs`, README (https://github.com/linebender/tiny-skia)
- Windows 레이어드 창: per-pixel alpha가 필요하면 `UpdateLayeredWindow` 사용. [검증] https://learn.microsoft.com/en-us/windows/win32/winmsg/window-features#layered-windows
- Tauri: macOS 투명 창은 `macOSPrivateApi: true`(Cargo feature `macos-private-api`) 필요, **App Store 불가**. [검증] tauri `crates/tauri-utils/src/config.rs` (https://github.com/tauri-apps/tauri/blob/dev/crates/tauri-utils/src/config.rs)
- Tauri 투명 창 관련 열린 버그 (모두 우리 용도와 직결) [검증]:
  - Windows 10에서 always-on-top 투명 창 + `set_ignore_cursor_events` 토글 시 투명 영역이 검게 렌더링됨. 신고자가 "desktop-pet style" 앱. https://github.com/tauri-apps/tauri/issues/15947
  - macOS에서 `transparent:true`면 정적 페이지여도 매 프레임 전체 재합성(GPU 전력 약 8배). https://github.com/tauri-apps/tauri/issues/15471
  - Linux/Nvidia 투명 창에서 크래시(GBM/Error 71) 또는 아티팩트. https://github.com/tauri-apps/tauri/issues/14924
- egui `ViewportBuilder::with_transparent`: "If this is not working, it's because the graphic context doesn't support transparency"; macOS는 고스팅 방지를 위해 `with_has_shadow(false)` 권장. [검증] egui 0.36.2 `src/viewport.rs`
- Slint: winit 백엔드가 Window 배경이 투명하면 `with_transparent(true)`로 생성. [검증] i-slint-backend-winit 1.18.1 `winitwindowadapter.rs`
- iced: `window::Settings { transparent, level, .. }`. [검증] iced_core 0.14.0 `src/window/settings.rs`

### 2.2 Always-on-top

- winit `set_window_level(WindowLevel::AlwaysOnTop)`: "just a hint to the OS". 구현 [검증] winit 0.30.13 소스:
  - macOS: `NSWindow.setLevel(kCGFloatingWindowLevel)` (`platform_impl/macos/window_delegate.rs`)
  - Windows: `WS_EX_TOPMOST` 플래그 (`platform_impl/windows/window.rs`, `window_state.rs`)
  - X11: `_NET_WM_STATE_ABOVE` 토글 (`platform_impl/linux/x11/window.rs`)
  - **Wayland: 빈 함수(no-op)** (`platform_impl/linux/wayland/window/mod.rs:430`)
- macOS 풀스크린 Space 위나 모든 Space에 표시하려면 `NSWindowCollectionBehavior.canJoinAllSpaces` 등 추가 설정 필요. winit에는 해당 API 없음 → objc2로 직접 호출. [검증: canJoinAllSpaces "The window can appear in all spaces" https://developer.apple.com/documentation/appkit/nswindow/collectionbehavior-swift.struct/canjoinallspaces] [추론: winit 0.30 `WindowExtMacOS`에 없음. 소스 grep 결과]
- 참고 구현 clawd-on-desk는 macOS에서 `type: "panel"`, Windows에서 `setAlwaysOnTop(true, level)` 재적용 루틴(`topmost-runtime.js`)을 둔다. Windows에서 topmost가 다른 topmost 앱에 밀리는 문제를 주기적으로 재적용한다는 의미. [검증: https://github.com/rullerzhou-afk/clawd-on-desk `src/pet-window-runtime.js`, `src/topmost-runtime.js`]

### 2.3 Click-through (핵심 난제)

**API 수준 [검증]**

| 스택 | API | 단위 | 구현 |
|---|---|---|---|
| winit 0.30 | `Window::set_cursor_hittest(bool)` | 창 전체 | macOS `setIgnoresMouseEvents(!hittest)`; Windows `WS_EX_TRANSPARENT \| WS_EX_LAYERED`; X11 XShape INPUT에 빈/전체 사각형; Wayland `wl_surface.set_input_region` 빈 영역/NULL |
| tao 0.37 / Tauri 2 | `set_ignore_cursor_events(bool)` | 창 전체 | Linux(GTK) `input_shape_combine_region` 1x1/None |
| egui/eframe 0.36 | `ViewportBuilder::with_mouse_passthrough`, `ViewportCommand::MousePassthrough` | 창 전체 | winit 경유 |
| iced 0.14 | `window::enable_mouse_passthrough` / `disable_…` | 창 전체 | winit 경유 |
| Slint 1.18 | 없음(`WinitWindowAccessor::with_winit_window`로 winit 접근) | 창 전체 | winit 경유 |

출처: winit 0.30.13 `src/window.rs`, `src/platform_impl/{macos,windows,linux/x11,linux/wayland}`; tao 0.37.0 `src/window.rs`, `src/platform_impl/linux/event_loop.rs:452`; egui 0.36.2 `src/viewport.rs`; iced_runtime 0.14.0 `src/window.rs`; i-slint-backend-winit 1.18.1 `lib.rs`.

winit 메인테이너 트래커에도 "window input areas" 추상화 요청이 열려 있다(2025-09-30). 본문: "On most desktop platforms you can just always have it be click transparent unless the global cursor position is over the area you want, but on Wayland this is problematic because Wayland (by design) presents no mechanism to get global cursor position, and stops sending cursor positions when you disable hittest." [검증] https://github.com/rust-windowing/winit/issues/4368

**플랫폼 네이티브 수단 [검증]**

- **Windows 레이어드 창**: "Hit testing of a layered window is based on the shape and transparency of the window. This means that the areas of the window that are color-keyed or whose alpha value is zero will let the mouse messages through. However, if the layered window has the WS_EX_TRANSPARENT extended window style, the shape of the layered window will be ignored and the mouse events will be passed to other windows underneath." https://learn.microsoft.com/en-us/windows/win32/winmsg/window-features#layered-windows
  → `UpdateLayeredWindow`로 프레임을 올리면 **알파 0 픽셀은 OS가 자동으로 통과**시킨다. 토글 루프가 필요 없다.
- **Windows `WM_NCHITTEST`/`HTTRANSPARENT`**: "In a window currently covered by another window in the same thread (the message will be sent to underlying windows **in the same thread** ...)". 다른 프로세스 창으로 클릭을 넘기지 못한다. https://learn.microsoft.com/en-us/windows/win32/inputdev/wm-nchittest
- **X11 SHAPE 확장**: 창마다 bounding/clip/**input** 영역이 있고, "The input region is the subset of the bounding region that can 'contain' the pointer". 영역 지정은 사각형 목록, 비트맵, 다른 창의 shape 중 하나로 한다. https://www.x.org/archive/current/doc/xextproto/shape.html
  → 알파 마스크에서 사각형 목록(또는 1비트 pixmap)을 만들어 `ShapeRectangles(SK::INPUT)`로 넣으면 네이티브 픽셀 단위 click-through가 된다. winit은 raw-window-handle로 XID를 노출하므로 x11rb로 직접 호출할 수 있다.
- **Wayland `wl_surface.set_input_region`**: "sets the region of the surface that can receive pointer and touch events. Input events happening outside of this region will try the next surface". 영역은 wl_region(사각형 합집합)이다. https://gitlab.freedesktop.org/wayland/wayland/-/blob/main/protocol/wayland.xml
- **macOS `NSWindow.ignoresMouseEvents`**: "A Boolean value that indicates whether the window is transparent to mouse events." 창 단위이고, 공식 문서에 per-pixel 동작은 설명이 없다. https://developer.apple.com/documentation/appkit/nswindow/ignoresmouseevents
- **macOS `NSEvent.mouseLocation`**: "Reports the current mouse position in screen coordinates ... regardless of the current event or pending events." https://developer.apple.com/documentation/appkit/nsevent/mouselocation

**권장 구현 [추론]**

1. 공통 추상화: `HitRegion` 트레이트 = `update(mask: &AlphaMask, window_pos)`.
2. Windows: 렌더 결과를 `UpdateLayeredWindow`로 제시 → 알파 기반 히트테스트 자동. 드래그 중 등은 그대로 동작.
3. X11: 프레임(또는 애니메이션 프레임 셋)별 알파 마스크 → 사각형 목록 캐시 → 프레임 전환 시 `ShapeRectangles(INPUT)`.
4. macOS: 30~60 Hz 타이머로 `NSEvent.mouseLocation`을 읽어 현재 프레임 알파 마스크를 조회하고, 변할 때만 `set_cursor_hittest` 토글. hittest가 꺼진 상태에서는 winit `CursorMoved`가 오지 않으므로 폴링이 필수.
   - 미확인: NSView `hitTest:`를 오버라이드해 nil을 반환하면 다른 앱 창으로 클릭이 넘어가는지는 공식 문서로 확인하지 못함 → 프로토타입 필요.
5. Wayland 네이티브: set_input_region이 가능하지만 winit은 빈/전체만 지원하고 raw `wl_surface`로 직접 호출해야 한다. 위치 지정·AOT가 없어 펫 용도에는 부적합 → XWayland로 대체.
6. 대안(2창 구조): clawd-on-desk는 시각 창과 별도로 히트박스 크기의 `hitWin`을 만들고 `setShape([{x:0,y:0,w,h}])`와 `setIgnoreMouseEvents`를 단일 writer로 관리한다("setShape: native hit region, no per-pixel alpha dependency"). [검증] clawd-on-desk `src/pet-window-runtime.js` L2323-2325
   - Rust로 옮길 경우: 렌더 창 `set_cursor_hittest(false)` 고정, hit 창은 undecorated·투명·AOT·작은 크기.
   - 주의 [추론]: Windows 레이어드 창에서 알파 0은 클릭이 통과하므로 hit 창은 알파 1/255 같은 거의 투명한 채움이 필요하다.

### 2.4 드래그 이동

- winit `Window::drag_window()`: 왼쪽 버튼이 "immediately before" 눌려 있어야 동작 보장. X11은 커서 그랩을 해제함, Wayland는 커서가 창 안에 있어야 함, macOS는 "May prevent the button release event to be triggered". [검증] winit 0.30.13 `src/window.rs` L1510-1525
- tao `drag_window`는 동일한 설명(macOS 경고 포함). Tauri는 `start_dragging`. [검증] tao 0.37.0 `src/window.rs` L1295-1305; https://docs.rs/tauri/latest/tauri/window/struct.Window.html
- iced `window::drag(id)`, egui `ViewportCommand::StartDrag`. [검증]
- [추론] 펫은 드래그 중 위치를 알아야 하므로(스냅, 화면 가장자리 처리) OS 드래그 대신 **수동 드래그**(`CursorMoved` + `set_outer_position`)가 더 제어하기 쉽다. macOS의 release 이벤트 누락 문제도 피한다. Wayland 네이티브에서는 `set_outer_position`이 미지원이라 불가 → XWayland에서만.

### 2.5 멀티 모니터 · DPI

- winit `outer_position`: "relative to the top-left hand corner of the desktop"; **Wayland: Always returns NotSupportedError**. `set_outer_position`: **Wayland Unsupported**. `current_monitor`, `available_monitors`, `scale_factor`, `WindowEvent::ScaleFactorChanged` 제공. [검증] https://docs.rs/winit/0.30.13/winit/window/struct.Window.html
- Win32 좌표: 멀티 모니터에서 x/y가 음수일 수 있다("Systems with multiple monitors can have negative x- and y- coordinates"). [검증] https://learn.microsoft.com/en-us/windows/win32/inputdev/wm-nchittest
- macOS 좌표계는 원점이 좌하단(Cocoa)이다. `NSEvent.mouseLocation`은 screen 좌표를 돌려주므로 winit의 물리 좌표로 변환해야 한다. [추론: 좌하단 원점은 AppKit 일반 지식이며 이번에 문서 인용은 하지 않음]
- Tauri에 "macOS physical position of windows and monitors reported incorrectly" 이슈가 열려 있다. [검증] https://github.com/tauri-apps/tauri/issues/7890

### 2.6 트레이 아이콘

- `tray-icon` 0.25.1 (Tauri 팀). [검증] README, https://docs.rs/tray-icon/0.25.1
  - Windows / Linux AppIndicator: "an event loop must be running on the thread". macOS: 메인 스레드에서 생성해야 함.
  - Linux 기본 백엔드는 GTK 3 + libxdo + libappindicator/libayatana-appindicator. **`ksni` feature**를 쓰면 StatusNotifierItem D-Bus 백엔드로 전환되고 "does not require these system libraries unless a muda GTK backend is also enabled", "manages its own worker thread".
- [추론] winit 앱에서는 Linux에 GTK 메인루프를 따로 돌리지 않도록 `ksni` 백엔드를 권장한다. 메뉴는 `muda`인데, GTK 없는 ksni 경로에서 메뉴가 어떻게 동작하는지(`muda-snapshot`)는 프로토타입으로 확인해야 한다.
- GNOME 기본 셸은 AppIndicator/SNI를 확장 없이는 표시하지 않는 것으로 알려져 있다. [추론, 1차 자료 미확인]

### 2.7 애니메이션 렌더링 경로

- GIF: `image::codecs::gif::GifDecoder`가 `AnimationDecoder`를 구현한다. [검증] image 0.25.10 `src/codecs/gif.rs:426`
- APNG: `PngDecoder::apng()` → `ApngDecoder`(`AnimationDecoder`), `is_apng()`. [검증] `src/codecs/png.rs:150-160,514`
- 애니메이션 WebP: `WebPDecoder`가 `AnimationDecoder`를 구현한다. [검증] `src/codecs/webp/decoder.rs:104`
- SVG: `resvg`(→ `usvg` 파싱, 의존성 `tiny-skia 0.12.0`)로 `Pixmap`에 래스터. [검증] resvg 0.48.1 `Cargo.toml`
  - SVG "애니메이션"(SMIL/CSS)은 resvg가 정적 렌더러이므로 지원하지 않는다고 판단 → 프레임별 SVG 또는 코드 트윈으로 대체. [추론, 확인 필요]
- 권장 파이프라인 [추론]:
  1. 로딩 시 모든 프레임을 프리멀티플라이드 RGBA `Pixmap`으로 디코드·캐시한다.
  2. 같은 시점에 프레임별 **알파 마스크/히트 사각형**도 미리 계산한다(click-through용).
  3. 프레임 타이머는 winit `ControlFlow::WaitUntil(next_frame)`. 유휴 시 `Wait`로 CPU를 0에 가깝게 유지한다.
  4. 프레젠트는 Windows `UpdateLayeredWindow`, X11 ARGB32 visual + `PutImage`(또는 MIT-SHM), macOS `CALayer.contents` = `CGImage`.
- clawd-on-desk 테마는 SVG/GIF/APNG 등 웹 에셋 기반이라 CSS 애니메이션 SVG가 있다면 호환성 공백이 생긴다. [추론, #7/#9 등 에셋 포맷 티켓과 연결]

### 2.8 Wayland 동작

- winit 0.30: Wayland에서 `set_window_level` no-op, `outer_position`/`set_outer_position` 미지원, `set_visible` 미지원("Android / Wayland / Web: Unsupported"), "Windows don't appear on Wayland until you draw/present to them". [검증] winit 0.30.13 소스·문서
- Wayland 강제 회피: `EventLoopBuilderExtX11::with_x11()` ("Force using X11"), 또는 `WAYLAND_DISPLAY` unset. `WINIT_UNIX_BACKEND`는 0.29에서 제거됐다. [검증] winit `src/platform/x11.rs`, `src/event_loop.rs:99`, `src/changelog/v0.29.md:134`
- XWayland에서는 X11 경로(XShape 입력 영역, `_NET_WM_STATE_ABOVE`, 절대 좌표)가 그대로 적용된다. 다만 AOT 준수와 네이티브 Wayland 창 위의 전역 커서 좌표는 컴포지터(Mutter/KWin) 재량이다. [추론]
- 네이티브 Wayland에서 제대로 하려면 `wlr-layer-shell`(winit 미지원) 같은 확장이 필요하고 GNOME은 지원하지 않는다. [추론, 1차 자료 미확인 → 별도 티켓 후보]

### 2.9 바이너리 크기 · 유휴 CPU (문서화된 것만)

- Tauri: "a minimal Tauri app can be less than 600KB in size" (시스템 웹뷰 사용). [검증] https://v2.tauri.app/start/
- Tauri macOS 투명 창은 정적이어도 지속 재합성(GPU ~8배). [검증] https://github.com/tauri-apps/tauri/issues/15471
- tiny-skia는 "binary size"를 목표로 명시한다. [검증] README
- winit/egui/iced/Slint의 크기·유휴 CPU 수치는 1차 자료에서 찾지 못했다 → 프로토타입 측정 필요. [미확인]
- [추론] wgpu는 셰이더 컴파일러(naga)와 백엔드를 포함해 수 MB가 추가되고 GPU 컨텍스트 메모리도 든다. 수백 px 스프라이트 하나에는 CPU 래스터로 충분하다.

## 3. 비교표

| 항목 | winit + CPU(tiny-skia) + 플랫폼 프레젠터 | winit + wgpu | Tauri 2 (tao+wry) | egui/eframe | Slint | iced |
|---|---|---|---|---|---|---|
| 투명 창 | 직접 구현(Win ULW / X11 ARGB / mac CALayer) [추론] | 가능, Windows는 DX12 `DxgiFromVisual` 필요, AMD+Vulkan 불가 [검증] | 가능, macOS private API(App Store 불가), Win10/Linux-Nvidia 버그 [검증] | winit+glow/wgpu 의존, 동일 제약 [검증] | winit 백엔드 투명 지원 [검증] | `transparent` 설정 [검증] |
| Always-on-top | winit X11/Win/mac OK, Wayland no-op [검증] | 동일 | `set_always_on_top` [검증] | `with_always_on_top` [검증] | `always-on-top` 속성 [검증] | `Level` [검증] |
| 픽셀 click-through | **Windows 네이티브(ULW 알파), X11 네이티브(XShape), mac 폴링 토글** [검증+추론] | 창 전체 토글 + 폴링만(ULW 불가) [추론] | 창 전체 토글 + 폴링; 토글 시 Win10 검은 화면 버그 [검증] | 창 전체 토글 [검증] | winit 접근해 토글 [검증] | 창 전체 토글 [검증] |
| 드래그 | `drag_window` 또는 수동 [검증] | 동일 | `start_dragging` [검증] | `StartDrag` [검증] | winit 경유 | `window::drag` [검증] |
| 멀티모니터/DPI | winit monitor API, Wayland 위치 불가 [검증] | 동일 | tao, macOS 좌표 버그 이슈 [검증] | winit | winit | winit |
| 트레이 | tray-icon(ksni) | 동일 | 내장(tray-icon 기반) | 별도 tray-icon | 별도 | 별도 |
| GIF/APNG/SVG | image + resvg [검증] | 동일 + 텍스처 업로드 | 브라우저 네이티브(최강) | image 로더 | 내장 이미지/SVG | image/svg 위젯 |
| Wayland | XWayland 강제 [검증] | 동일 | GTK 기반, 동일 제약 + Nvidia 버그 [검증] | winit | winit | winit |
| 무게 | 최소 [추론] | 중(wgpu) | 중(웹뷰 런타임) | 중 | 중 | 중 |
| 라이선스 | MIT/Apache | MIT/Apache | MIT/Apache | MIT/Apache | GPL-3 / Royalty-free / 상용 [검증 Cargo.toml] | MIT |

## 4. 권장안과 근거

**채택: winit 0.30.x + tiny-skia/resvg/image + 플랫폼 모듈(`windows-sys`, `x11rb`, `objc2-app-kit`) + tray-icon(`ksni`).**

근거:

1. 요구사항의 난제(픽셀 click-through, 투명 프레젠트)는 어느 툴킷도 해결해 주지 않는다. 결국 raw 핸들로 플랫폼 코드를 짜야 하며, winit은 raw-window-handle과 필요한 기본기(AOT, hittest 토글, drag, 모니터)를 모두 준다. [검증된 사실에 기반한 추론]
2. Windows에서 `UpdateLayeredWindow`는 알파 기반 히트테스트를 무료로 준다. 이 경로는 GPU 스왑체인(wgpu)과 양립하지 않고 CPU 비트맵(tiny-skia)과 자연스럽게 맞는다. [MS 문서 검증 + 추론]
3. X11 XShape 입력 영역은 사각형 목록으로 픽셀 단위 입력을 표현할 수 있다. 프레임별 마스크를 미리 계산하면 비용이 낮다. [X.org 스펙 검증 + 추론]
4. softbuffer는 릴리스 버전 기준 투명 불가(Win32/X11은 main에서도 미구현)라 현재는 채택 불가. 추후 릴리스되면 macOS/Wayland 프레젠터를 대체할 후보다. [검증]
5. Tauri는 에셋 호환성(SVG/CSS 애니메이션)에서 가장 강하지만, 투명·click-through 토글 관련 **데스크톱 펫 사용 사례의 열린 버그**와 macOS private API 요구가 있어 1순위에서 제외한다. 에셋 호환성이 결정적이라면 차선책. [검증]
6. winit 0.31은 beta(0.31.0-beta.3, 2026-09-04)라 API 변동 가능성이 있다. 0.30으로 시작하고 추상화 계층 뒤에 둔다. [검증: crates.io 버전 목록]

리스크 / 미확인 (프로토타입 필요):

- macOS에서 폴링 토글의 반응성, 그리고 hittest off→on 전환 직후 첫 클릭이 유실되는지.
- macOS에서 `CAMetalLayer` 없이 `CALayer.contents` CPU 프레젠트로 60fps 스프라이트 시 CPU 사용량.
- Windows `UpdateLayeredWindow`와 winit 창의 공존(winit이 WM_PAINT/레이어드 스타일을 건드리는지).
- XWayland에서 GNOME/KDE의 AOT, XShape 준수 여부.
- 2창(hit 창) 방식과 픽셀 방식 중 UX 선택.
