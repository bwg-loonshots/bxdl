# BXDL 제품 구현 설계

- 작성일: 2026-09-16 / 갱신: 2026-10-23
- 상태: Rust 기반의 package·제품 설정·setup, Mac 새 폴더 installer·NIGO cold, instance 등록·명시 init/resume-init과 수동 LaunchAgent start/status/stop과 오프라인 logs/diagnose를 구현했다. 단일 validator의 실제 서비스 수명주기를 확인했으며 setup 설치 연결도 구현했다. RC.1/RC.2의 고정 package 결과와 V2 자료 보관·시작 진단의 실제 인수를 보존하고, RC.3를 명시 선택해 같은 archive의 Mac 4-validator·원본 공급 경로 부재 setup13사례와 후속 전체 설치 UX21사례·독립 검사8항목을 통과했다. 전체 G1-M/G1-L 인수는 미완료다.
- 사용자 확정: **Rust 구현 + macOS Apple Silicon 우선 설치·운용 UX**. Linux 서버/systemd와 Docker/Compose는 후속 배포 대상으로 유지한다.
- 현재 구현 범위와 근거: [구현 상태](../docs/implementation-status.md). NIGO가 `REQ-0002`의 방향·범위를 수용했고 BXDL은 피드백의 안전·인수 의미와 교환 절차에 동의했다. [소비자 회신](./2026-09-17-nigo-feedback-response.md)에 기록하며 PROPOSED API·fixture·dirty 개발 JAR는 수신해 제한된 cold 소비를 진행했다. 이후 #119의 clean 후보를 별도 수신·검증했고 전체 제품 인수는 남아 있다. REQ-0003 정책을 수용하고 RC1을 별도로 수신했다. 요청 전체는 `OPEN`이며 소비자 회신은 전용 NIGO worktree에서 준비한다.

2026-10-20 RC.1의 정족수 복구 INCOMPLETE는 [당시 결과](../results/2026-10-20-macos-qbft-package.md)로 보존한다. 이후 [RC.2의 두 복구 경로](../results/2026-10-21-macos-rc2-package.md)와 [V2 자료 보관·시작 진단의13사례](../results/2026-10-22-macos-managed-supply.md)는 각각 지정 조합에서 PASS했다. [RC.2 전체 설치 UX](../results/2026-10-21-macos-rc2-setup.md)의 원본 A/B INCOMPLETE와 접근 경로 문제는 별도로 남아 있다.

2026-10-22 [RC.3 수신 검증](../contracts/nigo/engine-rc3-2026-10-22/README.md) 이후 [명시 선택과 Mac 재인수 설계](./2026-10-22-rc3-package-reacceptance.md)를 추가했다. 새 lock/spec으로 후보를 선택했고 같은 archive의 [4-validator 재인수](../results/2026-10-22-macos-rc3-package.md)와 [원본 부재 setup 13사례](../results/2026-10-22-macos-rc3-setup.md), 각 독립 사후 검사가 PASS했다. [NIGO PR #230](https://github.com/bwg-loonshots/nigo-protocol/pull/230) 정식 회신을 머지한 뒤 [같은 archive의 전체14문답·수정·취소/EOF·대기 중 HUP·재개21사례](../results/2026-10-22-macos-rc3-full-setup.md)와 독립 검사8항목도 PASS했다. 앞선 결과/회신의 당시 NOT_RUN은 보존한다. HUP은 대기 중 PTY 범위이며 mutation crash·Mac 로그아웃 인수가 아니다. offline Storage A7·기존 설치 upgrade·안전 제거·전체 G1-M/J5는 남아 있다.

## 현재 우선순위 — 직접 CLI

사용자가 console을 직접 확인한 뒤 개발 중단을 요청했다. **console 확장을 중단하고 CLI-01 → CLI-05를 진행한다.** [CLI 우선 설계](./2026-10-22-cli-simplification.md)와 [구현 계획](./2026-10-22-integrated-operations-ux-implementation-plan.md)이 현재 기준이다. 이름 연결/목록 → 한 줄 운용 → 순차 설치 준비 → 실제 설치 실행 → 자동완성 → 독립 관리 CLI 배포/기존 runtime 조합·사용자 과업 인수 순서다. 기존 번호 메뉴를 유지한 채 진입 명령만 짧게 만드는 것을 완료로 삼지 않는다.

기존 console·HTML·터미널 시안과 아래 검증 기록은 보존하되 추가 개발하지 않는다. 공통 저장·검증 API는 CLI에서 재사용하며, 2026-10-23 CLI-01의 `nodes`/`nodes add`를 구현했다. CLI-02의 [관리 CLI/기존 RC.3 실행기 분리](./2026-10-23-independent-management-cli.md)와 이름 기반 운용도 구현했다. [CLI-01 결과](../results/2026-10-23-cli01-node-names.md)와 [CLI-02 실제 조합 인수](../results/2026-10-23-independent-management-cli.md)를 따른다. CLI-03의 직접 `setup new/resume` 순차 문답, 읽기 전용 `setup list/show`, 제품 한 항목 수정 `setup edit`도 구현·검증했다. [준비 가이드](../docs/preparation.md)에 명령·저장·재검증 계약을, [CLI-03 결과](../results/2026-10-23-cli03-sequential-preparation.md)에 자동 검사와 실제 Mac 문답·조회 범위를 기록했다. CLI-04의 `setup apply`로 TTY 승인 뒤 설치·cold 검사·등록·이름 연결을 연결하고, 이름 기반 init/resume-init을 제공한다. 준비 `sessions/<이름>`과 실행 `operations/<이름>`을 구분하며 완료 재호출은 완료 근거 검증과 이름 복구만 수행한다. CLI-04 자동 검사와 최종 RC.3 Mac 12개 사례를 통과했다. [CLI-04 검증 기록](../results/2026-10-23-cli04-apply-and-lifecycle.md)을 따르며 CLI-05 중 zsh 자동완성(CLI05-01)의 구현·검증도 완료했다. [CLI05-01 검증 기록](../results/2026-10-23-cli05-zsh-completion.md)에 전체 검사·후속 보강·최종 release 실제 zsh 결과를 구분했다. [자동완성 가이드](../docs/completion.md)를 제공한다. CLI05-02 독립 Mac 배포·업데이트도 구현하고 [최종 검증](../results/2026-10-23-cli05-management-distribution.md)의 자동 검사·개발 인수 배포본 실제 signed13/unsigned13사례를 통과했다. 후속 [CLI05-03 조합 인수](../results/2026-10-23-cli05-runtime-acceptance.md)는 지정 Mac 서비스16사례와 기존 RC.3의4-validator·두 복구·wrong-pin·실행 중 CLI 전환3회·정상 정리를 PASS했다. 같은 source tree의 실제 PR head와 merge commit을 각각 빌드해 설치 진입점으로 운용했다. 진단 partial의 정확한 표현을 확인한 범위이며 **진단 완전수집은 OPEN**, CLI05-04 작성자가 아닌 사용자 과업은 NOT_RUN이다.

## 통합 설치·운용 UX 기존 설계·검증 이력 — 2026-10-22

당시 [통합 UX 설계안](./2026-10-22-integrated-operations-ux.md)은 별도 `bxdl console` 진입점에서 새 설치·이어하기·내 노드 관리·지원 보고서를 연결하는 방향이었다. Mac 설치·운용의 기능 인수와 처음 사용자의 사용성 인수는 구분한다. **UX-01 공통 문답과 UX-02 `console` 홈·목록·명시 기존 경로 읽기를 구현했다.** console의 추가 설치·운용 연결은 중단했으며 후속 기능은 위 CLI 계획에서 진행한다. [현재 console 사용 범위](../docs/console.md)와 [UX-02 검증 결과](../results/2026-10-22-ux02-console-catalog.md)를 따른다. 기존 CLI/JSON·저장 포맷·명시 확인·UNKNOWN 보존·LaunchAgent 범위를 유지한다.

UX-03 당시에는 `setup new/resume`과 console이 같은 번호 메뉴를 사용했다. CLI-03은 기존 저장 형식을 유지하며 직접 명령만 순차 문답으로 바꾸고 console 메뉴는 보존한다. 종전 UX-04~07의 후속 항목은 위 CLI-01~05로 재배치했다. 표시명·불변 ID·별칭을 구분하고 기존 명령/JSON을 유지한다. UX-03은 이름 기반 준비·재개·설정 출력을 구현했고 [로컬 검증](../results/2026-10-22-ux03-preparation.md)을 통과했다. [현재 가이드](../docs/preparation.md)는 CLI-03 흐름을 설명한다. 이름 기반 서비스 호출은 CLI-02에서 구현했고, 준비 세션에서 실제 설치로 연결하는 기능은 CLI-04로 별도 구현·검증한다. CLI-04 최종 인수는 완료했으며 CLI-05 사용자 과업을 후속으로 추적한다.

기존 [화면 명세](./2026-10-22-integrated-operations-ux-screens.md)와 [클릭 가능한 시안](./prototypes/integrated-operations-ux.html)은 당시 설계 이력이다. 다음 구현은 [현재 CLI-01~05 계획](./2026-10-22-integrated-operations-ux-implementation-plan.md)을 기준으로 한다. 안전 제거 REM-01과 나머지 Mac lifecycle은 G1-M의 별도 필수 항목이며 A7·업데이트는 G2로 구분한다. 이 계획 작성만으로 기존 요구 상태나 release gate를 완료 처리하지 않는다.

## 목표

Mac에서 반복 사용할 수 있는 설치·운용 UX를 먼저 구현한다. 고정된 NIGO 엔진을 담은 오프라인 설치 패키지로 영속 QBFT 노드를 구성하고, 정상 종료·동일 데이터 재시작·상태 확인·진단을 재현한다. 설치와 운영은 소스 checkout, Gradle, Rust/Cargo, Node/npm, GitHub token 또는 인터넷을 요구하지 않는다.

한 운영자가 관리하는 단위는 한 노드다. 네트워크 인수는 최소 4-validator mTLS 구성으로 수행한다. 한 노드 설치 완료를 단일 validator QBFT 네트워크 완성으로 표현하지 않는다.

## 읽기 순서

| 문서 | 내용 |
| --- | --- |
| [직접 CLI 간소화 설계](./2026-10-22-cli-simplification.md) | 순차 준비·재개·조회·제품 항목 수정, 설치 실행·이름 연결, 노드 별칭과 기존 CLI 호환 |
| [관리 CLI/노드 실행기 분리](./2026-10-23-independent-management-cli.md) | 기존 RC.3 유지, 관리 CLI 독립 업데이트와 실행기 검증·위임 |
| [독립 Mac 관리 CLI 배포·업데이트](./2026-10-23-management-cli-distribution.md) | CLI05-02 완료: CLI-only bundle·사용자 설치·버전 전환/복귀·셸 연결과 개발 배포본 실제 Mac 인수 |
| [CLI05-03 서비스·네트워크 인수](../results/2026-10-23-cli05-runtime-acceptance.md) | 지정 Mac 조합 PASS: 설치 진입점·실행 중 CLI 전환·고정 RC.3·정상 정리. 진단 완전수집 OPEN |
| [CLI 우선 구현 계획](./2026-10-22-integrated-operations-ux-implementation-plan.md) | 현재 CLI-01~05 상태·선행·검증 기준, 종전 UX 이력 |
| [통합 설치·운용 UX 설계안](./2026-10-22-integrated-operations-ux.md) | 새 진입점·자료/경로·상태·안전 경계·기존 CLI 호환 |
| [통합 UX 화면 명세](./2026-10-22-integrated-operations-ux-screens.md) | S01~S11 문구·입력·상태별 다음 행동 |
| [통합 UX 화면 시안](./prototypes/integrated-operations-ux.html) | 실제 명령을 실행하지 않는 화면·상태 탐색 모형 |
| [통합 UX 터미널 체험판](./prototypes/integrated-operations-ux-demo.md) | 번호·입력·개별 승인·운용·오류 동선을 메모리 예시로 체험하는 독립 데모 |
| [RC.3 선택·Mac 패키지 재인수](./2026-10-22-rc3-package-reacceptance.md) | 새 lock/spec·동일 archive의4-validator/원본 부재 setup 인수, A7 별도 |
| [등록 자료 보관·시작 진단](./2026-10-22-managed-supply-and-start-diagnostics.md) | V2 관리 사본·V1 유지·setup 재개·시작 단계 관측 |
| [RC.2 선택·Mac 패키지 재인수](./2026-10-21-rc2-package-reacceptance.md) | 지정 후보의 두 정족수 복구 경로와 기존 결과 보존 |
| [네트워크 설정 준비](./2026-10-19-network-preparation.md) | 공통 validator·peer/pin 입력, 기존 키 참조, RC1 native 생성·재개 |
| [시작 관측 UX](./2026-10-19-start-observation-ux.md) | 기한 종료·실제 실패 구분, 동일 시도의 읽기 대기 |
| [JRE 선정과 공급자 검증](./2026-10-19-jre-selection.md) | Temurin21 고정·전체 runtime staging·소스/legal·엔진 검증 분담 |
| [RC1 수신·Mac 패키지 인수](./2026-10-19-engine-rc1-adoption.md) | 후보 identity 고정, 등록 모드 안내·기존 계획 호환, 실제 signed package 인수 |
| [setup 설치 흐름](./2026-09-27-setup-install-flow.md) | 자료·계획·등록·명시 init/start와 중단·재개 |
| [오프라인 logs/diagnose](./2026-09-21-offline-diagnostics.md) | 최신 시도 이벤트·정제 지원 JSON·수집 예산·partial·안전한 파일 게시 |
| [macOS LaunchAgent](./2026-09-18-macos-launchagent.md) | 수동 bootstrap·일회 gate·start/status/stop·정상 종료 증명과 UNKNOWN 보존 설계 |
| [인스턴스 등록·초기화](./2026-09-18-instance-initialization.md) | private 등록 journal·명시 init/resume·동일 시도 결과 판정·UNKNOWN 보존 |
| [제품·엔진 설정 연결](./2026-09-18-product-engine-preflight.md) | clean 후보 수신 이후 v1 제품/native 설정 대조·cold 검사 UX |
| [Rust·macOS 우선 결정](./2026-09-17-rust-macos-first.md) | 최신 결정, 첫 UX 흐름, Mac profile과 Linux/Docker 확장 |
| [제품·배포 아키텍처](./2026-09-16-product-architecture.md) | 책임 경계, 기술 선택, 배포 layout, CLI·설정·상태·보안 모델 |
| [단계별 구현 계획](./2026-09-16-implementation-plan.md) | 작업 ID, 의존성, 산출물, 완료 기준, 구현 순서 |
| [NIGO 연동 계약](./2026-09-16-engine-integration.md) | 현행/제안 구분, NIGO-01~06 요청안, 공급자·소비자 책임 |
| [NIGO 피드백에 대한 소비자 회신](./2026-09-17-nigo-feedback-response.md) | 수신 revision·범위/안전 의미 수용·owner·상세 계약 및 공급 대기·결과 교환 |
| [패키지 인수·릴리스 계획](./2026-09-16-acceptance-and-release.md) | 테스트 matrix, 실패 주입, evidence, 릴리스 절차 |

## 결정 수준

| 구분 | 내용 |
| --- | --- |
| 사용자 확정 | 별도 BXDL 저장소, 기업용 허가형 제품, Rust 전환, macOS 우선 UX |
| 기반 구현 선택 | Rust 1.86.0와 Cargo.lock, development `.tar.gz`, Ed25519 서명·외부 key 검증, JSON 제품 설정 |
| 검증 대기 후보 | macOS arm64·Java 21·RocksDB·사용자 launchd 1차 인수, Linux/systemd 후속 |
| M0 잔여 결정 | 지원 macOS 최소 버전·launchd profile·자원, Temurin21 후보 선정 후 공급자 검증·재배포 자료 완성; Linux 조합은 후속 |
| NIGO 범위·의미 수용 | 첫 Mac NIGO-01~04/06, Linux·NIGO-05 후속, 초기화/검사/관측/종료 안전 의미와 결과 교환. 제공·수신 owner 확인 |
| NIGO 개발 후보 수신·잔여 | engine-info/preflight/init/resume-init/run과 runtime 계약/fixture·개발 JAR 수신. clean 후보와 QBFT 종료·live sync 보완 수신. BXDL 등록·초기화·수동 LaunchAgent 연결 이후 전체 제품 인수·공식 공급/유지보수 후속 |
| 후속 범위 | 원격 관리·SSO/RBAC, 새 웹 콘솔, fleet, Docker/Helm, 백업/복원 자동화, 인증서 순차 교체 |

추천안은 사용자가 기술 스택을 확정했거나 해당 플랫폼이 지원 검증을 통과했다는 뜻이 아니다. 구현자는 M0의 작은 검증으로 이 선택을 확정·수정하고 이유를 기록한다. 매 작업마다 같은 선택을 다시 논의하지 않는다.

## 검토한 기준

- NIGO 읽기 기준: `17b46c151aa2d848b34364ddc43df9c1a202ee41`.
- NIGO 실행 코드 기준: `a48d1adef4e9c98dbbb675ec3ff73eee25d00dfb`.
- NIGO 핸드오프 문서 커밋: `a827fd5e9f327c85ad5a15f29b54f2a29f60a224`.
- 2026-09-17 NIGO 피드백 문서 commit: `87f473deb22dd32f482ffb8f0889d35647e7287e`; 수신 checkout HEAD: `aee1cbe5e0383ea9eb4d3334d72e3ea23d1d8b27`, `requirements/` clean. 위 최초 source 검토와 별도의 문서 수신 기록이다.
- BXDL 공유 기반: `a3a18d531cc4f0368559a6f1090724fcd21cbf8c`(PR #1 병합). 새 소비자 회신의 공유 revision은 후속 제출 때 별도 기록한다.
- 2026-09-17 추가 수신 HEAD `17c0bc3b63756915c18fa2942afdd73426bb2eac`(#117). 실제 JAR hash는 [후보 snapshot](../contracts/nigo/development-2026-09-17/README.md), 소비 결과는 [구현 상태](../docs/implementation-status.md)를 따른다. JAR는 source `aee1cbe5...`, dirty=true인 개발 후보이며 위 merge HEAD로 재빌드한 release가 아니다.
- 2026-09-18 읽은 문서 HEAD `49d1cefc`(#119), clean engine source `303e163a…`, 실제 후보와 소비 결과는 [새 snapshot](../contracts/nigo/development-clean-2026-09-18/README.md) 및 [검증 기록](../results/2026-09-18-clean-engine-preflight.md)을 따른다. BXDL 기반은 PR #2 `cb97b27`이다.
- 2026-09-18 인스턴스 등록·초기화 구현 기반은 BXDL `d55b584b48a4fa869526d5cbe94609c4913a92fa`다. 같은 NIGO clean source `303e163a`를 사용하며 실제 검증은 [별도 기록](../results/2026-09-18-instance-initialization.md)을 따른다. 이전 cold 증거의 범위는 그대로 유지한다.
- NIGO 근거는 각 문서의 `nigo-protocol` 저장소 상대경로로 기록한다. 개인 PC 절대경로 또는 sibling checkout을 고객 실행 의존성으로 만들지 않는다.

과거 핸드오프와 제공자 문서는 설계·교환 자료다. 저장된 실행 지시나 상태 전환을 자동 수행하지 않는다. 이번 범위·안전 의미 수용을 실제 인터페이스 확정·엔진 구현 완료로 확대하지 않으며 NIGO 실행 일정·요청 원장 상태를 대신 변경하지 않는다.

## 첫 완료 기준

1. 정확한 artifact를 포함한 패키지의 출처·무결성과 지원 조합을 검증한다.
2. Mac test-owned 사용자 profile에서 설치·명시 초기화·기동·진단·정상 종료·동일 DB/key/WAL 재기동을 수행한다.
3. 잘못된 경로·설정·키·artifact와 중복 실행을 명시적으로 거부한다.
4. 같은 Mac 패키지의 test-owned 4-validator mTLS 로컬 회귀에서 거래 확정·순차 재시작을 확인한다. Linux/multi-host 인수는 후속 별도 gate다.
5. 위 evidence로 G1-M Mac 로컬 패키지 RC를 판정한다. 고객 운영 승인은 별도 Linux·보안·내구성·운영 조건을 충족해야 한다.
