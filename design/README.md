# BXDL 제품 구현 설계

- 작성일: 2026-09-16 / 갱신: 2026-09-18
- 상태: Rust 기반의 package·제품 설정·setup, Mac 새 폴더 installer·NIGO cold, instance 등록·명시 init/resume-init과 수동 LaunchAgent start/status/stop을 구현했다. 단일 validator의 실제 서비스 수명주기를 확인했으며 setup 설치 연결·전체 G1-M/G1-L 인수는 미완료다.
- 사용자 확정: **Rust 구현 + macOS Apple Silicon 우선 설치·운용 UX**. Linux 서버/systemd와 Docker/Compose는 후속 배포 대상으로 유지한다.
- 현재 구현 범위와 근거: [구현 상태](../docs/implementation-status.md). NIGO가 `REQ-0002`의 방향·범위를 수용했고 BXDL은 피드백의 안전·인수 의미와 교환 절차에 동의했다. [소비자 회신](./2026-09-17-nigo-feedback-response.md)에 기록하며 PROPOSED API·fixture·dirty 개발 JAR는 수신해 제한된 cold 소비를 진행했다. 이후 #119의 clean 후보를 별도 수신·검증했고 전체 제품 인수는 남아 있다. 요청 상태는 `OPEN`이고 이번에는 공유 중인 NIGO 원장을 변경하지 않는다.

## 목표

Mac에서 반복 사용할 수 있는 설치·운용 UX를 먼저 구현한다. 고정된 NIGO 엔진을 담은 오프라인 설치 패키지로 영속 QBFT 노드를 구성하고, 정상 종료·동일 데이터 재시작·상태 확인·진단을 재현한다. 설치와 운영은 소스 checkout, Gradle, Rust/Cargo, Node/npm, GitHub token 또는 인터넷을 요구하지 않는다.

한 운영자가 관리하는 단위는 한 노드다. 네트워크 인수는 최소 4-validator mTLS 구성으로 수행한다. 한 노드 설치 완료를 단일 validator QBFT 네트워크 완성으로 표현하지 않는다.

## 읽기 순서

| 문서 | 내용 |
| --- | --- |
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
| M0 잔여 결정 | 지원 macOS 최소 버전·launchd profile·자원, JRE 공급자·patch·hash·재배포 자료; Linux 조합은 후속 |
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
