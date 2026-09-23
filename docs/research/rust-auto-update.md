# Rust 데스크톱 앱 자동 업데이트·배포 조사

- 티켓: sunwjy/claude-pet#4 (map: #1)
- 조사일: 2026-09-23
- 대상: MIT/Apache-2.0 개인 오픈소스 Rust GUI 앱(투명 always-on-top 창 + 트레이), macOS / Windows / Linux X11. GUI 스택 미정(winit 계열, Tauri, egui, Slint 후보).

표기 규칙: **[사실]**은 인용한 1차 출처에서 확인한 내용이고, **[추론]**은 그 사실들을 바탕으로 한 필자의 판단이다.

---

## 요약

1. **배포 도구와 업데이터는 GUI 스택과 별개로 고를 수 있다. 단, Tauri 업데이터만은 예외다.** Tauri 업데이터(`tauri-plugin-updater`)는 Tauri 번들러 결과물(`.app.tar.gz`, AppImage, MSI/NSIS)과 한 묶음이다 [사실, 출처 T1]. Tauri를 쓰지 않으면 같은 팀(CrabNebula)이 만든 프레임워크 무관 버전 `cargo-packager` + `cargo-packager-updater`가 있지만, 마지막 릴리스가 2025-11이다 [사실, P1].
2. **GUI 앱 번들(.app, 설치형 exe, AppImage)을 만들고 자체 업데이트까지 되는 프레임워크 무관 Rust 도구 가운데 현재 가장 활발한 것은 Velopack이다.** Rust SDK가 있고, `GithubSource`를 지원하며, Windows Setup.exe와 macOS .pkg/.zip(.app), Linux AppImage를 만들고, delta 업데이트도 된다 [사실, V1–V5]. 2026-09-21에 릴리스가 나왔다 [사실, R1].
3. **cargo-dist(dist)와 axoupdater는 CLI 배포용이다.** dist는 macOS dmg/.app을 "Future Installers"로 분류한다(미지원) [사실, D1]. 업데이터도 shell/PowerShell 설치본의 install receipt를 기준으로 동작한다 [사실, D2, D3]. 따라서 GUI 펫 앱에는 맞지 않는다 [추론].
4. **self_update 크레이트는 1.0.0부터 macOS `.app` 디렉터리 번들을 통째로 교체하는 모드가 있다** [사실, S1, S2]. 설치기(.msi/.deb)는 다루지 않고 서명·공증도 하지 않는다 [사실, S1]. 번들링은 별도 도구로 하고 업데이트만 직접 구현하는 조합에 쓸 수 있다.
5. **Sparkle/WinSparkle**은 성숙한 도구지만 Rust에서 쓰기는 어렵다. Sparkle은 macOS 전용 Objective-C 프레임워크이고, Rust 바인딩은 2026-09에 나온 `sparkle-updater` 0.1.0(다운로드 수십 건)과 Tauri 플러그인 정도다 [사실, R2]. Linux 대응도 따로 필요하다.
6. **서명·공증 비용이 실질적인 병목이다.**
   - macOS: Developer ID 서명과 공증에는 Apple Developer Program($99/년)이 필요하다. 개인은 수수료 면제 대상이 아니다 [사실, A1, A2]. 공증하지 않은 앱은 Sequoia부터 Control-클릭으로 우회할 수 없고, 시스템 설정 > 개인정보 보호 및 보안에서 "그래도 열기"를 눌러야 한다 [사실, A3, A4]. Homebrew 공식 cask는 2026-09부터 Gatekeeper 검사에 실패하는 cask를 비활성화한다 [사실, H1, H2].
   - Windows: 서명하지 않으면 SmartScreen이 "Windows protected your PC" 경고를 띄우고 사용자가 "Run anyway"를 눌러야 한다. EV 인증서도 더는 SmartScreen을 우회하지 못한다 [사실, M1]. Azure Artifact Signing($9.99/월~)은 **개인 개발자의 경우 미국·캐나다 거주자만** 이용할 수 있다. 한국은 조직(법인)만 가능하다 [사실, M2, M3]. SignPath Foundation은 OSI 라이선스 OSS에 무료로 서명해 주지만 게시자 이름이 SignPath Foundation으로 표시되고, 프로젝트 평판 심사가 있다 [사실, SP1].
7. **권장안:** 초기(v0.x)에는 **Velopack + GitHub Releases**로 3 OS 설치본과 자동 업데이트를 한 번에 해결하고, 처음에는 **서명 없이** 배포한다. 이때 경고 우회 절차를 README에 적는다. macOS 사용자가 늘면 Apple Developer Program($99/년)으로 서명·공증을 붙이고, Windows는 SignPath Foundation 신청을 우선 검토한다. Tauri를 GUI 스택으로 고른다면 Tauri 번들러와 업데이터를 쓰는 편이 가장 마찰이 적다. 이 경우 업데이터 서명 키(minisign 계열)는 필수다 [추론].

---

## 1. 후보별 상세

### 1.1 dist (구 cargo-dist) + axoupdater

- **상태:** v0.33.0(2026-09-10) 릴리스, Apache-2.0, 활발히 개발 중 [사실, R1].
- **설치 형식:** shell(`curl | sh`), PowerShell(`irm | iex`), npm, Homebrew formula, MSI [사실, D1]. "macOS dmg / app", "macOS cask", "windows winget package", "linux flatpak"은 **Future Installers**(요청은 있으나 일정 없음)로 분류되어 있다 [사실, D1].
- **업데이트 흐름:** `install-updater = true`로 설정하면 `<app>-update`라는 독립 바이너리가 함께 설치된다. 이 기능은 **shell/PowerShell 설치기**에서 동작하고 "currently experimental"이다 [사실, D2]. axoupdater는 dist가 남긴 **install receipt**(`~/.config/APP` 또는 `%LOCALAPPDATA%\APP`)를 읽어 현재 버전을 판단하며, GitHub Releases와 Axo Releases를 지원한다 [사실, D3].
- **서명:** Windows는 SSL.com eSigner를 지원하고, v0.33.0부터 Azure Artifact Signing도 지원한다(x86_64 Windows 대상 한정) [사실, D4]. macOS codesign은 "experimental"이다 [사실, D4]. 공증 지원은 확인하지 못했다(미확인).
- **GUI 앱 적합성:** .app 번들, dmg, AppImage를 만들지 못한다 [사실, D1]. 업데이트도 receipt 기반 CLI 설치본을 전제로 한다 [사실, D3]. 따라서 **데스크톱 펫에는 부적합**하다. 다만 GitHub Actions 릴리스 파이프라인과 체크섬, attestation을 자동화하는 데는 참고할 만하다 [추론].

### 1.2 self_update 크레이트

- **상태:** 1.3.0(crates.io 2026-09-02), MIT, 최근 다운로드 약 235만 건 [사실, R2]. (GitHub Releases 페이지는 2018년 이후 갱신되지 않았지만 crates.io 배포는 계속되고 있다 [사실, R1, R2].)
- **백엔드:** GitHub(기본), GitLab, Gitea, Gitee, S3 호환 스토리지, 정적 매니페스트 [사실, S1].
- **검증:** `signatures` 기능(zipsign으로 `.zip`/`.tar.gz` 서명 검증)과 `checksums` 기능 [사실, S1].
- **GUI 번들:** 1.0.0에서 "Directory-bundle installs (macOS `.app`)"가 추가됐다 [사실, S2]. `bundle_path_in_archive("MyApp.app")`를 지정하면 스테이징한 번들 전체를 원자적으로 rename하고, 실패하면 롤백한다. `restart()`로 재실행할 수 있다 [사실, S1].
- **한계 [사실, S1]:**
  - "The crate never signs, notarizes, or staples"
  - App Translocation(격리 속성이 붙은 채 실행된 앱)에서는 `Error::AppTranslocated`가 난다.
  - Windows에서는 번들 안의 DLL을 열고 있으면 교체에 실패한다.
  - ".deb / .msi packages are a different shape entirely": 시스템 설치기로 설치한 경우는 직접 `msiexec`/`dpkg`를 호출해야 한다.
- **GUI 앱 적합성:** 업데이트 로직만 제공한다. 설치본(.app 생성, dmg, Windows 설치기, AppImage)은 cargo-packager나 cargo-bundle 같은 **별도 번들러**가 필요하다. 업데이트 UI(알림, 동의)도 직접 만들어야 한다 [추론].

### 1.3 Tauri 번들러 + updater 플러그인

- **상태:** `tauri-plugin-updater` 2.12.0(2026-09-21), Apache-2.0/MIT [사실, R2].
- **업데이트 흐름:** 정적 JSON(예: GitHub Releases에 올린 `latest.json`)이나 동적 서버에서 버전과 플랫폼별 URL, 서명을 가져온다 [사실, T1].
- **서명:** 업데이트 서명은 **필수이고 끌 수 없다**("This cannot be disabled") [사실, T1].
- **업데이트 가능한 형식:** Linux는 AppImage, macOS는 `.app.tar.gz`, Windows는 MSI 또는 NSIS(.exe)다. Windows 설치 모드는 passive/basicUi/quiet [사실, T1]. (.deb는 목록에 없다 → Linux 자동 업데이트는 AppImage만 된다 [추론].)
- **OS 서명:** macOS는 Apple Developer 유료 계정이 있어야 공증할 수 있다. ad-hoc 서명(`-`)은 Apple Silicon에서 실행은 되지만 "does not prevent MacOS from requiring users to whitelist the installation in their Privacy & Security settings" [사실, T2]. Windows는 OV 인증서, Azure Key Vault, Azure Artifact Signing, 사용자 지정 `signCommand`를 지원한다. EV/OV는 이제 SmartScreen 평판을 같은 방식으로 쌓는다 [사실, T3].
- **GUI 스택 결합:** Tauri 앱(= WebView UI)을 전제로 한다. Tauri를 쓰지 않는 앱에서 이 플러그인만 쓰는 것은 공식 경로가 아니다 [추론]. 프레임워크 무관 대안인 **cargo-packager**(dmg/.app, deb/AppImage/pacman, NSIS/MSI) + **cargo-packager-updater**(서명 공개키 필수, `{{target}}/{{arch}}/{{current_version}}` 엔드포인트)가 있다 [사실, P1, P2]. 다만 마지막 릴리스가 cargo-packager 0.11.8(2025-11-27), updater 0.2.3(2025-07-21)이어서 유지보수 속도를 확인해야 한다 [사실, R2].

### 1.4 Velopack

- **상태:** 1.2.158(2026-09-21), MIT, Rust SDK는 "Ready" 상태 [사실, V1, R1].
- **통합:** `main()` 첫 줄에서 `VelopackApp::build().run()`을 호출하고, `UpdateManager`로 `check_for_updates` → `download_updates` → `apply_updates_and_restart`를 실행한다. GUI 프레임워크 요구사항은 없다 [사실, V2].
- **소스:** `GithubSource`, `GitlabSource`, `GiteaSource`, `HttpSource`, `FileSource`, `VelopackFlowSource` [사실, V3]. 따라서 GitHub Releases를 바로 쓸 수 있다.
- **산출물 [사실, V4, V5, V6]:**
  - Windows: Setup.exe와 Portable ZIP, 전체 `.nupkg` 및 delta `.nupkg`
  - macOS: `.pkg` 설치기와 `.app`이 든 portable `.zip`(DMG는 직접 만들 수 있다)
  - Linux: 설치기 없이 `.AppImage` 하나. 업데이트할 때는 AppImage를 교체하고, 권한이 필요한 위치면 `pkexec`로 권한을 올린다.
- **서명:** 문서는 "Code signing and notarization is required by Apple before shipping your releases to users, or your app won't run"이라고 적는다 [사실, V5]. 즉 서명·공증 자체는 여전히 개발자 몫이다. CLI 옵션 이름은 이번에 확인하지 못했다(미확인).
- **GUI 앱 적합성:** 3 OS 모두 GUI 번들 형태의 설치와 자동 업데이트를 한 도구로 해결한다. 프레임워크 무관이라 winit/egui/Slint에 모두 쓸 수 있다 [추론]. 단점은 .NET 기반 `vpk` CLI가 필요하다는 점 [미확인]과, 최근 다운로드 약 2.9만 건으로 Tauri나 self_update보다 Rust 생태계 채택이 적다는 점이다 [사실, R2].

### 1.5 Sparkle / WinSparkle

- **Sparkle(macOS):** 2.10.0(2026-09-13) [사실, R1]. appcast(RSS)와 EdDSA(ed25519) 서명, `SUFeedURL`/`SUPublicEDKey` Info.plist 키를 쓴다. 비 Xcode 프로젝트도 rpath 설정으로 지원한다. Developer ID 서명과 공증을 강하게 권장한다 [사실, SK1].
- **WinSparkle(Windows):** 0.9.4(2026-07-21), MIT [사실, R1]. Sparkle과 같은 appcast 형식에 EdDSA 서명을 쓰고, C API를 제공한다. 공식 바인딩은 C#, Python, Go, Pascal이다 [사실, SK2]. 업데이트 파일로 설치기(예: `Updater.exe`)를 받아 실행하는 방식이다 [사실, SK2].
- **Rust 바인딩:** crates.io의 `sparkle` 크레이트는 Servo GL 바인딩으로 무관하다. `sparkle-updater` 0.1.0(2026-09-19, 최근 다운로드 19건)과 `tauri-plugin-sparkle-updater` 0.3.0이 있다. WinSparkle용 Rust 크레이트는 crates.io 검색에서 찾지 못했다 [사실, R2].
- **적합성:** Linux 대응이 없고, 두 네이티브 라이브러리를 FFI로 붙여야 한다. 1인 프로젝트에는 비용이 크다 [추론].

### 1.6 OS 패키지 매니저 (Homebrew cask, winget)

- **Homebrew cask:** 공식 `homebrew/cask`는 "Gatekeeper checks를 통과해야" 한다 [사실, H1]. Homebrew 5.0.0은 "We will disable all Homebrew/homebrew-cask casks that fail Gatekeeper checks in September 2026"라고 발표했고, `--no-quarantine`을 deprecate했다 [사실, H2]. 공식 cask에는 notability 기준도 있다 [사실, H1]. → **서명·공증하지 않은 앱은 공식 cask에 올릴 수 없다.** 개인 tap은 가능하지만 사용자는 여전히 Gatekeeper 경고를 거쳐야 한다 [추론]. 또 Homebrew는 cask가 앱의 자체 업데이트와 공존할 수 있다고 설명한다("self-updating applications can replace themselves outside Homebrew") [사실, H3].
- **winget:** EXE(Silent 플래그 필요), ZIP, INNO, NULLSOFT, MSI, WIX, MSIX, PORTABLE 등을 지원한다 [사실, W1]. 저장소 정책에는 서명 필수 조항이 없고, `InstallerUrl`이 ISV의 릴리스 위치(예: GitHub Releases)여야 한다 [사실, W2]. 제출은 `microsoft/winget-pkgs`에 매니페스트 PR을 보내는 방식이다 [사실, W3].
- **역할:** 둘 다 "업데이터"가 아니라 **배포 채널**이다. 사용자가 `brew upgrade`나 `winget upgrade`를 실행해야 업데이트된다 [추론].

---

## 2. 서명·공증 요구사항과 비용

| 항목 | macOS | Windows | Linux |
|---|---|---|---|
| 필요 조건 | Developer ID 서명 + hardened runtime + `notarytool` 공증 + `stapler` [사실, A5] | Authenticode 서명(OV/EV 또는 Artifact Signing) [사실, M1] | 없음(AppImage 서명은 선택) [추론] |
| 비용 | Apple Developer Program **$99/년**. 개인은 면제 불가 [사실, A1, A2] | Artifact Signing **$9.99/월**(Basic)부터 [사실, M1]. 개인은 **미국·캐나다만** 가능 [사실, M3]. SignPath Foundation은 OSS 무료 [사실, SP1] | $0 |
| 미서명 시 | Gatekeeper가 차단한다. Sequoia부터 Control-클릭 우회가 없어져 시스템 설정 > 개인정보 보호 및 보안 > "그래도 열기"를 눌러야 한다(약 1시간 안) [사실, A3, A4]. Apple Silicon은 ad-hoc 서명이라도 있어야 실행된다 [사실, T2] | SmartScreen "Windows protected your PC" → "More info" → "Run anyway". 미서명이면 버전마다 평판이 0부터 다시 쌓인다. Win11 Smart App Control은 평판 없는 미서명 파일을 차단한다 [사실, M1] | 영향 없음 |
| 서명해도 | 공증하면 경고 없음 [사실, A5] | 새 인증서나 새 앱은 평판이 쌓일 때까지 여전히 경고가 뜬다("several weeks and hundreds of clean installs"). EV도 우회 불가 [사실, M1] | — |

추가 사실:
- Artifact Signing은 무료·체험·후원 구독에서는 쓸 수 없고 유료 Azure 구독이 필요하다. 인증서에는 검증된 법적 이름이 들어가며 CN을 바꿀 수 없다 [사실, M2].
- SignPath Foundation 조건: OSI 라이선스(상업 이중 라이선스 없음), 활발히 유지보수 중이고 이미 릴리스된 프로젝트, 검증 가능한 자동 빌드, 저장소 소유 팀이 서명을 담당할 것. 게시자는 SignPath Foundation으로 표시된다 [사실, SP1]. macOS 지원 여부는 확인하지 못했다(미확인).

**한국 거주 1인 개발자라는 조건에서의 판단 [추론]:**
- Windows 서명 경로는 사실상 SignPath Foundation(무료, 심사 필요)이나 상용 OV 인증서(클라우드 HSM/토큰, 연 수십만 원대. 이번 조사에서 가격은 확인하지 않았다) 둘 중 하나다. Artifact Signing은 개인 자격으로는 이용할 수 없다.
- 서명하더라도 초기에는 SmartScreen 경고를 피할 수 없다. 반면 macOS는 $99만 내면 공증으로 경고가 완전히 사라진다. 따라서 비용 대비 효과는 macOS 쪽이 크다.

---

## 3. 비교표

| 방식 | 업데이트 흐름 | GitHub Releases | 산출 형식 (mac / win / linux) | GUI 번들 | 업데이트 서명 검증 | GUI 스택 결합 | 유지보수 (2026-09) |
|---|---|---|---|---|---|---|---|
| **dist + axoupdater** | 별도 `<app>-update` 실행 또는 라이브러리. install receipt 기반 | O (기본) | tarball+shell, Homebrew formula / PowerShell, MSI / tarball+shell | **X** (dmg/.app 미지원) | 체크섬/attestation | 없음 | 활발 (0.33.0) |
| **self_update** | 앱 안에서 호출 → 아카이브를 받아 바이너리나 .app 디렉터리 교체 → `restart()` | O (기본) | 번들러 별도 필요 (tar.gz/zip 입력) | 부분 (.app 교체 O, 설치기 X) | zipsign(선택), 체크섬 | 없음 | 활발 (1.3.0) |
| **Tauri updater** | JSON 매니페스트 → 다운로드 → 설치기/번들 교체 | O (정적 JSON) | .app.tar.gz(+dmg) / MSI, NSIS / AppImage(+deb는 업데이트 X) | O | **필수** | **Tauri 전용** | 활발 (2.12.0) |
| **cargo-packager(+updater)** | Tauri updater와 같은 모델 | O (엔드포인트) | dmg, .app / NSIS, MSI / deb, AppImage, pacman | O | 필수 (공개키) | 없음 | 느림 (2025-11) |
| **Velopack** | `UpdateManager` → 다운로드(delta) → 적용·재시작 | O (`GithubSource`) | .pkg, .zip(.app) / Setup.exe, portable zip / AppImage | O | (미확인) | 없음 | 활발 (1.2.158) |
| **Sparkle + WinSparkle** | appcast RSS → 다운로드 → 설치 | 호스팅만 가능 | dmg/zip / 설치기 exe / **없음** | O | EdDSA 필수 | 없음(FFI 필요) | 라이브러리는 활발, Rust 바인딩은 미성숙 |
| **Homebrew cask / winget** | 사용자가 `brew`/`winget upgrade` 실행 | URL 참조 | .app(dmg/zip) / exe, msi, zip | O | sha256 | 없음 | — (cask는 공증 필수) |

---

## 4. 권장안 [추론]

1. **기본 조합:** 번들링과 업데이트는 **Velopack**, 호스팅은 **GitHub Releases**, 빌드와 업로드는 GitHub Actions에서 OS별 러너로 한다.
   - 3 OS 모두 GUI 번들 형태 설치본과 앱 안 자동 업데이트를 한 도구로 해결하고, GUI 스택 결정과 독립적이다.
   - 대안: GUI를 **Tauri**로 정하면 Tauri 번들러와 `tauri-plugin-updater`로 바꾼다. 업데이트 서명 키를 관리해야 한다.
   - 차선(최소 의존): `cargo-packager`로 번들링하고 `self_update`(bundle 모드 + zipsign)로 업데이트한다.
2. **서명은 단계적으로:**
   - v0.x: 서명하지 않는다. README에 macOS "그래도 열기" 절차와 Windows "Run anyway" 절차를 안내한다. macOS arm64는 최소 ad-hoc 서명을 한다.
   - 사용자가 생기면: Apple Developer Program($99/년)으로 Developer ID 서명과 공증을 붙이고, Homebrew **공식 cask** 등록을 검토한다(공증이 전제).
   - Windows: SignPath Foundation을 신청한다(프로젝트가 "이미 릴리스되어 있고 활발"해야 하므로 v0.x 릴리스 이후). 거절되면 상용 OV 인증서를 검토한다. 등록 비용이 없는 winget은 언제든 추가할 수 있다.
   - Linux: AppImage 하나로 시작한다. deb, Flatpak 등은 수요가 생기면 추가한다.
3. **dist는 쓰지 않는다.** CLI 설치기 중심이라 GUI 펫에는 맞지 않는다. 다만 CI 템플릿과 attestation 패턴은 참고한다.

---

## 5. 새로 떠오른 질문 (티켓 후보)

- Velopack의 macOS 서명·공증 CLI 옵션과 CI 흐름 검증(`vpk`가 macOS 러너에서 notarytool을 호출하는지, .NET 의존성이 있는지).
- 투명 always-on-top 창과 트레이를 쓰는 앱에서 Velopack의 `VelopackApp::build().run()`과 업데이트 후 재시작이 이벤트 루프(winit 등)와 충돌하지 않는지 프로토타입으로 확인.
- SignPath Foundation 신청 요건(최소 릴리스와 평판 기준, macOS 지원 여부)과 한국 개인이 쓸 수 있는 상용 OV 코드 서명 가격 비교.
- 사용자 동의 없이 백그라운드 업데이트를 할지, 트레이 메뉴 "업데이트 확인"으로 할지 등 업데이트 UX 정책.
- Linux X11에서 AppImage만으로 충분한지(트레이/AppIndicator 의존성, FUSE 요구사항).

---

## 출처

- **R1** GitHub 릴리스 목록 (`gh release list`, 2026-09-23 조회): axodotdev/cargo-dist, axodotdev/axoupdater, jaemk/self_update, velopack/velopack, sparkle-project/Sparkle, vslavik/winsparkle
- **R2** crates.io API (2026-09-23 조회): https://crates.io/api/v1/crates/{self_update, axoupdater, velopack, tauri-plugin-updater, cargo-packager, cargo-packager-updater, sparkle}, 검색 `q=sparkle`, `q=winsparkle`
- **D1** dist Installers: https://axodotdev.github.io/cargo-dist/book/installers/index.html (소스 `book/src/installers/index.md` "Future Installers")
- **D2** dist Updater: https://axodotdev.github.io/cargo-dist/book/installers/updater.html
- **D3** axoupdater README: https://github.com/axodotdev/axoupdater
- **D4** dist CHANGELOG: https://github.com/axodotdev/cargo-dist/blob/main/CHANGELOG.md (0.33.0 Azure Artifact Signing, SSL.com, "experimental macOS codesigning")
- **S1** self_update README: https://github.com/jaemk/self_update (섹션 "Bundle installs (macOS `.app`)", "signatures")
- **S2** self_update CHANGELOG 1.0.0: https://github.com/jaemk/self_update/blob/master/CHANGELOG.md
- **T1** Tauri Updater plugin: https://v2.tauri.app/plugin/updater/
- **T2** Tauri macOS Code Signing: https://v2.tauri.app/distribute/sign/macos/
- **T3** Tauri Windows Code Signing: https://v2.tauri.app/distribute/sign/windows/
- **P1** cargo-packager README: https://github.com/crabnebula-dev/cargo-packager
- **P2** cargo-packager-updater README: https://github.com/crabnebula-dev/cargo-packager/tree/main/crates/updater
- **V1** Velopack docs: https://docs.velopack.io/
- **V2** Velopack Rust 시작하기: https://docs.velopack.io/getting-started/rust
- **V3** velopack crate sources: https://docs.rs/velopack/latest/velopack/sources/index.html
- **V4** Velopack Packaging overview: https://docs.velopack.io/packaging/overview
- **V5** Velopack macOS: https://docs.velopack.io/packaging/operating-systems/macos
- **V6** Velopack Linux: https://docs.velopack.io/packaging/operating-systems/linux
- **SK1** Sparkle documentation: https://sparkle-project.org/documentation/
- **SK2** WinSparkle README: https://github.com/vslavik/winsparkle
- **A1** Apple Developer Program – What's included: https://developer.apple.com/programs/whats-included/
- **A2** Apple fee waivers: https://developer.apple.com/help/account/membership/fee-waivers
- **A3** Apple Developer News (Sequoia Gatekeeper): https://developer.apple.com/news/?id=saqachfa
- **A4** Apple Support – Open a Mac app from an unknown developer: https://support.apple.com/guide/mac-help/open-a-mac-app-from-an-unknown-developer-mh40616/mac
- **A5** Notarizing macOS software before distribution: https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution
- **M1** SmartScreen reputation for Windows app developers: https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/smartscreen-reputation
- **M2** Artifact Signing FAQ: https://learn.microsoft.com/en-us/azure/artifact-signing/faq
- **M3** Artifact Signing Quickstart (Prerequisites): https://learn.microsoft.com/en-us/azure/artifact-signing/quickstart
- **SP1** SignPath Foundation terms: https://signpath.org/terms (및 https://signpath.org/)
- **H1** Homebrew Acceptable Casks: https://docs.brew.sh/Acceptable-Casks
- **H2** Homebrew 5.0.0 발표: https://brew.sh/2025/11/12/homebrew-5.0.0/
- **H3** Homebrew Security and Supply Chain ("Casks have a different trust model"): https://docs.brew.sh/Homebrew-Security-and-Supply-Chain
- **W1** WinGet supported installer formats: https://learn.microsoft.com/en-us/windows/package-manager/winget/
- **W2** Windows Package Manager repository policies: https://learn.microsoft.com/en-us/windows/package-manager/package/windows-package-manager-policies
- **W3** Submit packages to Windows Package Manager: https://learn.microsoft.com/en-us/windows/package-manager/package/
