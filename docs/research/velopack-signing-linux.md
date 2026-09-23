# Velopack 서명·Linux 배포 세부 조사

- 티켓: sunwjy/claude-pet#15 (map: #1, 선행 조사: #4 `docs/research/rust-auto-update.md` @ `research/rust-auto-update`)
- 조사일: 2026-09-23
- 대상: 개인 OSS Rust 데스크톱 펫(투명 always-on-top 창 + 트레이), macOS / Windows / Linux X11. 개발자는 한국 거주 개인.

표기 규칙: **[사실]**은 인용한 1차 출처에서 확인한 내용이고, **[추론]**은 그 사실을 바탕으로 한 필자의 판단이다. #4에서 이미 확인한 내용(Apple $99/년, Azure Artifact Signing의 개인 자격 제한 등)은 반복하지 않는다.

---

## 요약 (질문별 답)

1. **Velopack이 CI에서 macOS 서명·공증을 할 수 있는가? .NET이 필요한가?**
   가능하다. `vpk pack`에 `--signAppIdentity`, `--signInstallIdentity`, `--notaryProfile`, `--keychain` 옵션을 주면 `codesign`(deep 서명, hardened runtime entitlements)과 `xcrun notarytool` 공증, `stapler`까지 자동으로 한다. 공식 문서에 GitHub Actions 예제(p12를 Secrets로 두고 임시 keychain 사용)도 있다. 다만 macOS 패키지는 **macOS 러너에서만** 만들 수 있다. **.NET은 빌드 머신(CI)에만 필요하다.** `vpk`는 .NET global tool이어서 .NET SDK가 있어야 한다. 앱 런타임에는 필요 없다. Rust SDK(`velopack` 크레이트)와 Update/Setup 바이너리는 Rust로 작성되어 있다.
2. **SignPath Foundation 요건, macOS 지원, 한국 개인용 OV 가격**
   - SignPath Foundation: OSI 라이선스(상업 이중 라이선스 불가), 독점 코드 없음, 활발한 유지보수, **이미 릴리스된 상태**, 다운로드 페이지에 기능 설명, CI에서 검증 가능하게 빌드, 릴리스마다 수동 승인, 팀 전원 MFA, 홈페이지에 "Code signing policy" 명시, 그리고 **검증 가능한 평판**(언론·블로그·다운로드 수 등)이 필요하다. 인증서 명의는 "SignPath Foundation"이다.
   - macOS: SignPath.io 플랫폼 자체는 macOS `codesign`/`productsign`을 지원한다. 그러나 **본인의 Apple Developer ID 인증서가 있어야** 한다. SignPath Foundation이 무료로 주는 인증서는 Windows Authenticode용이므로 macOS 경고를 없애 주지 못한다 [추론, 근거는 §2.2].
   - 한국 거주 개인이 살 수 있는 인증서: **Certum Open Source Code Signing(클라우드 €49, 카드 세트 €69)**이 가장 저렴하다. 그다음은 Certum Standard(클라우드 €209), **SSL.com IV($129/년 + eSigner 클라우드 월 $20~)**다. Sectigo와 DigiCert 직판은 연 $500 이상이고, 개인 자격 여부가 페이지에 명시되어 있지 않다.
3. **Linux X11에서 AppImage 하나로 충분한가?**
   **조건부로 충분하다.** Velopack 1.2.158은 type2 런타임(정적 링크)을 쓴다. 그래서 호스트에 `libfuse2`가 없어도 된다. 다만 FUSE 커널 모듈과 `fusermount`/`fusermount3` 바이너리는 필요하다. 또 Velopack은 AppDir에 공유 라이브러리를 번들하지 않는다. 따라서 트레이는 **`tray-icon`의 `ksni` 백엔드(순수 Rust D-Bus StatusNotifierItem, GTK·libappindicator 불필요)**를 쓰는 편이 AppImage 하나로 배포하기에 안전하다. SNI 호스트가 없는 환경(확장을 끈 GNOME, 순수 XEmbed 트레이만 있는 WM)에서는 어느 백엔드를 쓰든 트레이가 보이지 않는다. 그러므로 트레이 없이도 동작하도록 설계해야 한다.

---

## 1. Velopack의 macOS 서명·공증과 .NET 의존성

### 1.1 서명·공증 흐름 [사실]

- Velopack 문서는 서명을 Velopack이 직접 해야 한다고 적는다: "signing needs to be performed by Velopack itself, this is because the Velopack binaries (such as Update and Setup) need to be signed at different points in the package build process" [V-SIGN].
- 준비물 [V-SIGN]:
  - Apple Developer 계정(연회비)
  - **`Developer ID Application`과 `Developer ID Installer` 인증서 두 개**
  - notarytool 프로필(앱 전용 암호 + Team ID)
- 옵션 [V-SIGN, V-CLI-OSX]: `--signAppIdentity`, `--signInstallIdentity`, `--notaryProfile`, `--keychain`, `--signEntitlements`, `--signDisableDeep`. 각 옵션에는 `VPK_SIGN_APP_IDENTITY` 같은 환경 변수가 대응한다.
  - identity에는 팀 이름을 붙이지 않는다(`Developer ID Application: Your Name`) [V-SIGN].
- 동작은 소스(`OsxPackCommandRunner.cs`)에서 확인했다 [V-SRC-OSX]:
  - Velopack이 추가하는 `UpdateMac`을 먼저 서명하고, 이어서 `.app` 전체를 `--deep`으로 서명한다.
  - `--signAppIdentity`와 `--notaryProfile`이 둘 다 있으면 zip으로 묶어 `Notarize`한 뒤 `Staple`한다.
  - identity만 있으면 "signed but not notarized" 경고를 낸다.
  - 둘 다 없으면 "Package will not be signed or notarized" 경고를 내고 계속 진행한다. 즉 **미서명 빌드도 만들 수 있다**.
  - `.pkg` 설치기는 `--signInstallIdentity`와 `--notaryProfile`이 있으면 서명하고 공증한다.
- entitlements를 지정하지 않으면 "one suitable for most dotnet apps"를 기본으로 쓴다 [V-SIGN]. App Sandbox는 지원하지 않는다 [V-MAC].
- **GitHub Actions 예제** [V-SIGN]: `macos-latest` 러너를 쓰고 Secrets 7개(`BUILD_CERTIFICATE_BASE64`, `INSTALLER_CERTIFICATE_BASE64`, `P12_PASSWORD`, `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM`, `KEYCHAIN_PASSWORD`)를 둔다. 흐름은 임시 keychain을 만들고 `security import`로 p12를 넣은 뒤 `xcrun notarytool store-credentials`를 실행하고, `dotnet tool install -g vpk` 후 `vpk pack ... --keychain $RUNNER_TEMP/app-signing.keychain-db`를 호출하는 것이다.
- **크로스 빌드 제한** [V-CROSS]:
  - "It is not possible to create MacOS packages on Windows or Linux" (`codesign`, `xcrun`, `productbuild`에 의존하기 때문).
  - Windows와 Linux 패키지는 어느 OS에서나 만들 수 있다.
  - 단 Windows **서명**은 `signtool.exe`가 필요하므로 Windows 러너에서 하거나 JSign(`--signTemplate`)을 써야 한다.

### 1.2 .NET 의존성 [사실]

- 빌드 시: "`vpk` ... is distributed as a .NET global tool ... the .NET SDK is required to install and run `vpk`" [V-INSTALL]. 설치는 `dotnet tool install -g vpk` 또는 `dnx vpk@<version>`으로 한다. 문서는 앱이 쓰는 Velopack 라이브러리와 **같은 버전의 vpk**를 쓰라고 권한다 [V-INSTALL].
  - GitHub 호스티드 러너(macOS/Windows/Ubuntu)에는 보통 .NET SDK가 미리 설치되어 있다 [추론, 이번에 확인 안 함].
- Windows에서 Azure Artifact Signing을 쓰면 서명 머신에 .NET 8 런타임이 추가로 필요하다 [V-SIGN]. 한국 개인은 Artifact Signing을 쓸 수 없으므로(#4) 해당 사항이 없다.
- 런타임: Velopack 저장소에서 Rust SDK(`src/lib-rust`)와 Update/Setup/Stub 바이너리(`src/bins`: `update.rs`, `setup.rs`, `stub.rs`)는 Rust이고, C#은 `vpk` CLI(`src/vpk`)뿐이다 [V-REPO]. README도 "Velopack is written in Rust"라고 적는다 [V-REPO]. 따라서 **Rust 앱 사용자에게 .NET은 필요 없다** [추론, 근거는 소스 구조].

### 1.3 참고: Velopack 문서와 Microsoft 문서의 불일치

- Velopack 서명 문서는 "EV certificates are granted instant SmartScreen reputation"이라고 적는다 [V-SIGN].
- 반면 Microsoft 1차 문서(#4의 M1)는 EV도 더는 SmartScreen을 우회하지 못한다고 명시한다. **Microsoft 문서를 우선한다** [추론].

---

## 2. Windows 코드 서명 경로 (한국 거주 개인)

### 2.1 SignPath Foundation 가입 요건 [사실, SP-TERMS, SP-FORM]

- **프로젝트 조건**
  - 악성 코드와 PUP가 없어야 한다.
  - 모든 구성요소가 OSI 승인 라이선스여야 하고, 상업 이중 라이선스는 안 된다.
  - 독점 코드가 없어야 한다(시스템 라이브러리는 예외).
  - "actively maintained", "already be released in the form that should be signed", 다운로드 페이지나 앱스토어에 기능이 설명되어 있어야 한다.
- **Foundation 인증서 사용 조건**
  - 인증서 명의가 SignPath Foundation이므로 게시자로 SignPath Foundation이 표시된다.
  - 자기 저장소에서 자기 소스로 빌드한 바이너리만 서명할 수 있다.
  - 해킹 도구는 안 된다.
  - 사용자 데이터를 전송하면 개인정보 처리방침을 두고 설치 중에 보여 주고 끌 수 있게 해야 한다.
  - 설치를 제공하면 제거 방법도 제공해야 한다.
- **팀·보안**
  - 전원 MFA(SignPath와 GitHub 모두).
  - Authors/Reviewers/Approvers 역할을 정한다. 외부 PR은 리뷰해야 하고 **릴리스마다 수동 승인**을 거친다.
- **웹사이트**
  - 홈페이지와 다운로드 페이지에 "Code signing policy" 섹션을 두고, "Free code signing provided by SignPath.io, certificate by SignPath Foundation", 팀 역할, 개인정보 처리방침(또는 정해진 문구)을 적는다.
- **기술 제약**
  - 서명 대상 바이너리의 product name과 version 메타데이터를 강제한다.
  - "Binary artifacts must be built from source code in a verifiable way"(지원 CI 예: GitHub Actions, AppVeyor).
- **평판 심사**
  - "For executable programs ... we require a certain verifiable reputation. (Not for developer libraries ...)".
  - 신청서의 Reputation 칸에 언론 보도, 블로그, Wikipedia, 다운로드·포크 수, GitHub Insights 등을 적는다.
  - 프로젝트 이름은 구글 검색 최상단에 나와야 한다("Ideally").
  - "We're under no obligation to accept your project, and there is no independent arbitration mechanism".
- 신청은 signpath.org/apply(HubSpot 폼)에서 한다. 필드는 공개 양식 `OSSRequestForm-v4.xlsx`와 같다.
- 개인 신원 확인은 요구하지 않는다("No need for personal identification") [SP-HOME].

→ [추론] 펫 앱은 "Program"으로 분류되므로 평판 심사를 받는다. **첫 릴리스 직후에는 통과하기 어렵다.** 다운로드 수나 외부 언급이 쌓인 뒤 신청하는 것이 현실적이다. 또 CI 서명 단계에 **릴리스마다 사람의 승인**이 들어간다.

### 2.2 SignPath와 macOS [사실 + 추론]

- SignPath.io 문서는 macOS CryptoTokenKit 크립토 프로바이더로 `codesign`/`productsign`을 지원한다. 단 "codesign requires an 'Apple Developer Application' certificate", "productsign requires an 'Apple Developer Installer' certificate"라고 적고, 공증은 다루지 않는다 [SP-MAC].
- 아티팩트 구성 레퍼런스의 지원 형식은 PE, MSI, APPX/MSIX, JAR, APK, RPM, DEB, ZIP, OCI 등이고, macOS 번들 형식은 없다 [SP-REF].
- SignPath Foundation 약관에 따르면 Foundation은 CA가 아니고, CA가 "SignPath Foundation" 명의로 발급한 인증서를 프로젝트가 쓰게 해 준다 [SP-TERMS].
- → [추론] Developer ID 인증서는 Apple이 Apple Developer Program 회원에게만 발급한다. 따라서 **SignPath Foundation의 무료 인증서로는 macOS Gatekeeper나 공증 문제를 해결할 수 없다.** macOS는 본인의 Apple Developer Program($99/년)이 여전히 필요하다.

### 2.3 한국 거주 개인이 살 수 있는 코드 서명 인증서 (2026-09-23 공식 페이지 기준)

배경 [사실]:
- 2023-06-01부터 코드 서명 키는 HSM(USB 토큰 또는 클라우드 HSM)에 보관해야 한다. 파일로 내려받을 수 없다 [V-SIGN].
- 2026-02-27부터 최대 유효기간이 459일로 줄어서, 다년 구매도 매년 재발급해야 한다 [CERTUM-SHOP].

| CA / 상품 | 가격 | 대상 | 키 보관 · CI | 출처 |
|---|---|---|---|---|
| **Certum Open Source Code Signing (클라우드)** | **€49** | 개인 전용("issued only for individuals"). 공개된 OSS 프로젝트와의 연관 증빙 필요. 인증서 주체는 "Open Source Developer, 이름" | SimplySign 클라우드. 모바일 앱 OTP와 SimplySign Desktop 필요. 월 5,000회 서명 한도. CI 자동화 여부는 페이지에 명시 없음 | [CERTUM-SHOP], [CERTUM-OSS], [CERTUM-DOCS] |
| Certum Open Source (카드 세트 / 코드만) | €69 / €25 | 같음 | 물리 카드와 리더기(배송). CI에는 부적합 [추론] | [CERTUM-SHOP] |
| Certum Standard Code Signing | 클라우드 €209, 세트 €169, 코드 €139 | 소프트웨어 게시자·개발자 | SimplySign 또는 카드 | [CERTUM-SHOP] |
| **SSL.com IV (Individual Validation)** | **$129/년**(5년 선결제 시 $96.75/년) + 키 보관 비용 | "Independent developers, open-source maintainers", "no business registration required" | eSigner 클라우드: 월 $20(20회), $85(100회) 등. CI/CD API 사용 가능. 또는 YubiKey +$379 | [SSLCOM-IV], [SSLCOM-ESIGNER] |
| Sectigo OV (직판) | 5년 선택 시 연 $536.25부터 | 페이지에 "organization or individual" 문구가 있으나 개인 절차는 명시 없음 | 토큰 배송 또는 자체 FIPS HSM | [SECTIGO] |
| DigiCert Standard (직판) | 월 $44 표기(연 결제 금액 표기가 페이지에서 일관되지 않음). 연 $500 이상 | 개인 자격 명시 없음 | KeyLocker 클라우드, 토큰, HSM | [DIGICERT] |

비고:
- Certum 신원 확인 방법 [CERTUM-DOCS]: 자동 신원 확인(권장), 등록처, 공증, 또는 신분증(ID card, **passport**, 운전면허, 영주권)을 든 사진. 공과금 고지서도 필요하다.
  - **국가 제한 문구는 없다.** 한국어 서류의 번역 필요 여부도 명시되어 있지 않다(미확인).
- SSL.com IV 1년에 eSigner 최저 티어(월 $20)를 더하면 연 약 $369다. 릴리스가 적으면 달마다 구독을 켜고 끄는 식으로 비용을 줄일 수 있는지는 확인하지 못했다(미확인).
- → [추론] **가성비 1순위는 Certum Open Source 클라우드(€49/년)**다. 다만 SimplySign은 모바일 OTP 기반이라 GitHub Actions에서 완전 자동으로 서명하기 어렵다. 공식 자동화 경로는 확인하지 못했다. **CI 자동화가 중요하면 SSL.com IV + eSigner**가 현실적이다. 어느 쪽이든 OV/IV 인증서는 평판이 쌓일 때까지 SmartScreen 경고가 남는다(#4, M1).

---

## 3. Linux X11: AppImage 하나로 충분한가

### 3.1 Velopack이 만드는 AppImage [사실]

- Linux에서는 설치기 없이 `.AppImage` 하나만 만든다. 사용자는 받은 뒤 `chmod +x`하고 실행한다. 업데이트할 때는 `/var/tmp`에 받은 다음 AppImage 파일을 교체하고, 권한이 필요한 위치면 `pkexec`로 권한을 올린다 [V-LINUX].
- 소스 `LinuxPackCommandRunner.cs` [V-SRC-LINUX]:
  - pack 디렉터리를 `usr/bin`에 그대로 복사한다.
  - `AppRun`(셸 스크립트, `PATH`만 설정하고 `LD_LIBRARY_PATH`는 설정하지 않음)과 `.desktop`, 아이콘, `UpdateNix`를 추가한다.
  - linuxdeploy류의 **공유 라이브러리 수집이나 번들링은 없다.**
- 소스 `AppImageTool.cs` [V-SRC-APPIMAGE]: `mksquashfs`로 squashfs를 만들고 vendor의 AppImage 런타임 뒤에 이어 붙인다. Linux나 macOS 빌드 머신에는 `squashfs-tools`가 필요하다.
- vendor 런타임은 2026-05-26 커밋 "update to appimage type2 runtimes"에서 **AppImage/type2-runtime**(continuous, 2026-03-07)으로 바뀌었다 [V-COMMIT, V-NOTICES]. 이 커밋은 최신 릴리스 1.2.158(2026-09-21)에 포함되어 있다(`compare` API로 확인).

### 3.2 FUSE 요구사항 [사실]

- type2-runtime: "It mounts the payload via FUSE", "Since the runtime is linked statically, libfuse2 is no longer required on the target system" [T2RT].
- 런타임 소스(`runtime.c`) [T2RT-SRC]:
  - `$PATH`에서 setuid root인 `fusermount*` 바이너리를 찾는다. 없으면 "No suitable fusermount binary found" 오류를 낸다.
  - 마운트에 실패하면 "Cannot mount AppImage, please check your FUSE setup"을 출력한다. **자동으로 추출 실행으로 넘어가지 않는다.**
  - `APPIMAGE_EXTRACT_AND_RUN=1` 환경 변수나 `--appimage-extract-and-run`을 주면 FUSE 없이 임시로 추출해서 실행한다.
- AppImage 문서의 FUSE 문제 해결 항목은 여전히 `libfuse2`(Ubuntu 24.04에서는 `libfuse2t64`) 설치를 안내한다 [AI-FUSE]. → [추론] 이 안내는 구형(type2 이전 AppImageKit) 런타임용이다. Velopack 1.2.x 결과물 사용자는 `libfuse2` 대신 **FUSE 커널 모듈과 `fusermount`/`fusermount3`**만 있으면 된다. 최신 데스크톱 배포판(Ubuntu 22.04+ 등 fuse3 기본 설치)은 대부분 이 조건을 만족한다. 컨테이너나 최소 설치 환경에서는 `APPIMAGE_EXTRACT_AND_RUN=1`을 안내한다.

### 3.3 glibc 호환성 [사실 + 추론]

- AppImage 모범 사례 문서는 앱이 돌아갈 가장 오래된 베이스 시스템보다 새로운 시스템에서 빌드하지 말라고 한다. glibc는 빌드한 시스템보다 오래된 시스템에서 깨진다 [AI-BP].
- → [추론] Velopack은 라이브러리를 번들하지 않는다(§3.1). 따라서 Rust 바이너리가 링크한 glibc 버전이 곧 최소 요구 배포판을 정한다. CI는 가능한 한 오래된 러너(예: `ubuntu-22.04`)에서 빌드한다.

### 3.4 트레이 의존성 [사실]

- `tray-icon` 0.25.x(2026-09-16) [TRAY, TRAY-CL]:
  - Linux 백엔드는 "AppIndicator or KSNI" 두 가지다.
  - 기본 `libappindicator` 기능은 "GTK 3, `libxdo`, and `libappindicator` or `libayatana-appindicator`"가 필요하다.
  - 0.25.0에서 "GTK-free `ksni` StatusNotifierItem backend"가 추가됐다. "The `ksni` backend does not require these system libraries unless a muda GTK backend is also enabled". 두 기능을 다 켜면 ksni를 쓴다.
  - 기본 features는 `["muda-libxdo", "libappindicator"]`다 [TRAY-CARGO]. 따라서 ksni만 쓰려면 `default-features = false, features = ["ksni"]`처럼 기본값을 꺼야 libxdo와 GTK를 링크하지 않는다 [추론, Cargo.toml 근거].
  - "The KSNI backend manages its own worker thread". AppIndicator 백엔드는 해당 스레드에서 (GTK) 이벤트 루프가 돌아야 한다 [TRAY].
- `libappindicator` 크레이트(tray-icon의 기본 백엔드)는 `libayatana-appindicator3.so.1` → `libappindicator3.so.1` 순서로 **런타임에 dlopen**한다 [LAI-SRC]. → [추론] 빌드는 되더라도, 사용자 시스템에 이 라이브러리와 GTK3가 없으면 트레이가 실패한다. Velopack AppImage는 이 라이브러리를 번들하지 않으므로 호스트에 의존하게 된다.
- `ksni` 0.3.6(2026-07-15): "A Rust implementation of the KDE/freedesktop StatusNotifierItem specification" [KSNI]. D-Bus만 쓰는 순수 Rust 구현이다.
- 트레이가 **보이는지**는 데스크톱 환경의 SNI 호스트가 결정한다. GNOME Shell은 "AppIndicator and KStatusNotifierItem Support" 확장이 있어야 AppIndicator/SNI를 표시한다 [GNOME-EXT]. (Ubuntu는 이 확장을 기본으로 켜 두고, 순정 GNOME은 그렇지 않다 [추론, 이번에 확인 안 함].) KDE Plasma는 SNI를 기본 지원한다 [추론]. XEmbed 트레이만 있는 WM(i3bar 등)은 SNI 아이콘을 보여 주지 않는다 [추론].

### 3.5 결론 [추론]

- **배포 형식은 AppImage 하나로 시작해도 된다.** FUSE는 대부분의 데스크톱에 이미 있고, 없으면 `APPIMAGE_EXTRACT_AND_RUN=1`이라는 우회로가 있다.
- 단 AppImage가 "자기완결적"이려면 앱 바이너리가 GTK나 libappindicator 같은 호스트 라이브러리에 기대지 않아야 한다. 그러므로 **트레이는 `tray-icon` + `ksni` 기능(또는 `ksni` 직접 사용)**을 권장한다. 창은 winit(X11은 libX11/libxcb를 dlopen) 계열로 두는 편이 번들 문제가 가장 적다.
- README에 적을 것:
  - GNOME 사용자는 AppIndicator 확장이 필요하다.
  - FUSE 오류가 나면 `APPIMAGE_EXTRACT_AND_RUN=1`을 쓴다.
  - AppImageLauncher 같은 통합 도구를 쓸 수 있다.
- deb나 Flatpak은 수요가 생기면 검토한다.

---

## 4. 권장 CI 구성 스케치 [추론]

- `macos-latest`: `cargo build`(universal 또는 arch별) → `vpk pack`(미서명 단계에서는 서명 옵션 생략, 이후 `--signAppIdentity`, `--signInstallIdentity`, `--notaryProfile`, `--keychain`) → `vpk upload github`.
- `windows-latest`: `vpk pack`. 인증서를 확보하면 `--signParams`(signtool, SSL.com eSigner CKA 등)나 `--signTemplate`을 쓴다. SignPath를 쓰면 SignPath의 GitHub Action으로 서명 요청을 보내고 승인을 기다리는 단계가 들어간다.
- `ubuntu-22.04`: `squashfs-tools`를 설치하고 `vpk pack`(`--icon` PNG 필수) 실행.
- 세 러너 모두 .NET SDK와 `vpk`(앱의 `velopack` 크레이트와 같은 버전)가 필요하다.

---

## 5. 새로 떠오른 질문

- Certum SimplySign을 GitHub Actions에서 무인으로 서명할 수 있는가(공식 CLI나 API 여부)? 이것이 SSL.com eSigner와의 선택을 가른다.
- `tray-icon` `ksni` 백엔드와 winit 투명 창을 함께 쓸 때 X11에서 메뉴와 클릭 이벤트가 정상인지, 그리고 GTK를 완전히 빼고 빌드되는지 프로토타입 검증.
- Velopack이 기본 제공하는 hardened runtime entitlements("for most dotnet apps")가 Rust 앱에 과하거나 부족하지 않은지. 투명 창과 always-on-top에는 특별한 entitlement가 필요 없을 것으로 보이지만 확인이 필요하다.
- SignPath Foundation 평판 기준을 충족하려면 어느 정도 규모(다운로드 수, 외부 언급)가 필요한지. 공개 기준은 없고 심사자 재량이다.

---

## 출처 (2026-09-23 조회)

- **V-SIGN** Velopack Code Signing: https://docs.velopack.io/packaging/signing (소스 https://github.com/velopack/velopack.docs/blob/master/docs/packaging/signing.mdx)
- **V-CLI-OSX** vpk macOS CLI 레퍼런스: https://github.com/velopack/velopack.docs/blob/master/docs/reference/cli/content/vpk-osx.mdx
- **V-INSTALL** vpk 설치: https://github.com/velopack/velopack.docs/blob/master/docs/getting-started/content/_install-vpk.mdx
- **V-CROSS** Cross Compiling: https://docs.velopack.io/packaging/cross-compiling
- **V-MAC** MacOS Overview: https://docs.velopack.io/packaging/operating-systems/macos
- **V-LINUX** Linux Overview: https://docs.velopack.io/packaging/operating-systems/linux
- **V-REPO** velopack 저장소(develop 브랜치 트리, README): https://github.com/velopack/velopack
- **V-SRC-OSX** https://github.com/velopack/velopack/blob/develop/src/vpk/Velopack.Packaging.Unix/Commands/OsxPackCommandRunner.cs
- **V-SRC-LINUX** https://github.com/velopack/velopack/blob/develop/src/vpk/Velopack.Packaging.Unix/Commands/LinuxPackCommandRunner.cs
- **V-SRC-APPIMAGE** https://github.com/velopack/velopack/blob/develop/src/vpk/Velopack.Packaging.Unix/AppImageTool.cs
- **V-COMMIT** https://github.com/velopack/velopack/commit/4a9313628374af6aaef354d81507d9a915b12af0
- **V-NOTICES** https://github.com/velopack/velopack/blob/develop/vendor/THIRD_PARTY_NOTICES.md
- **SP-TERMS** SignPath Foundation terms: https://signpath.org/terms
- **SP-HOME** https://signpath.org/
- **SP-FORM** 신청 양식: https://signpath.org/apply, https://signpath.org/assets/OSSRequestForm-v4.xlsx (사이트 소스 https://github.com/SignPath/fdn-website)
- **SP-MAC** SignPath macOS CryptoTokenKit: https://docs.signpath.io/crypto-providers/macos
- **SP-REF** SignPath artifact configuration reference: https://docs.signpath.io/artifact-configuration/reference
- **CERTUM-SHOP** https://shop.certum.eu/data-safety/code-signing-certificates.html
- **CERTUM-OSS** https://shop.certum.eu/open-source-code-signing-on-simplysign.html
- **CERTUM-DOCS** https://support.certum.eu/en/code-signing-required-documents/
- **SSLCOM-IV** https://www.ssl.com/products/software-integrity/code-signing/iv/
- **SSLCOM-ESIGNER** https://www.ssl.com/guide/esigner-pricing-for-code-signing/
- **SECTIGO** https://www.sectigo.com/ssl-certificates-tls/code-signing
- **DIGICERT** https://www.digicert.com/signing/code-signing-certificates
- **T2RT** https://github.com/AppImage/type2-runtime (README)
- **T2RT-SRC** https://github.com/AppImage/type2-runtime/blob/main/src/runtime/runtime.c
- **AI-FUSE** https://docs.appimage.org/user-guide/troubleshooting/fuse.html (소스 https://github.com/AppImage/docs.appimage.org/blob/master/source/user-guide/troubleshooting/fuse.rst)
- **AI-BP** https://docs.appimage.org/reference/best-practices.html
- **TRAY** tray-icon README: https://github.com/tauri-apps/tray-icon
- **TRAY-CL** tray-icon CHANGELOG 0.25.0: https://github.com/tauri-apps/tray-icon/blob/dev/CHANGELOG.md
- **TRAY-CARGO** https://github.com/tauri-apps/tray-icon/blob/dev/Cargo.toml
- **LAI-SRC** https://github.com/tauri-apps/libappindicator-rs/blob/main/sys/src/lib.rs
- **KSNI** https://github.com/iovxw/ksni, https://crates.io/crates/ksni
- **GNOME-EXT** https://github.com/ubuntu/gnome-shell-extension-appindicator
