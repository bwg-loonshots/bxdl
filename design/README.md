# BXDL 설계 문서

BXDL은 NIGO 엔진의 패키징·설치·초기화·서비스 운용을 담당하는 Rust CLI다. 현재 개발·실물 검증의 우선 대상은 **실제 Linux 서버**이며, 사용자 흐름은 `setup`과 노드 이름을 사용하는 직접 명령을 중심으로 구성한다. Mac의 사용자 LaunchAgent 경로와 검증된 RC.3·RC.4 패키지 지원은 유지한다.

현재 제공 기능과 검증 수준은 [구현 상태](../docs/implementation-status.md)와 [지원 범위](../docs/support-matrix.md)를 따른다. 이 폴더의 날짜가 붙은 문서는 해당 시점의 설계 결정·변경 근거다. 과거 문서의 “다음 단계”, “미구현”, “Mac 우선”을 현재 제품 상태로 해석하지 않는다.

## 현재 설계의 기준

- **관리 CLI와 노드 실행기를 분리한다.** 관리 CLI의 업데이트·복귀가 설치된 NIGO JAR·JRE·고정 실행기·DB를 교체하지 않는다. 지원하는 패키지나 descriptor를 검증한 뒤 해당 실행기에 운용을 위임한다.
- **준비·설치·초기화·시작을 나눈다.** `setup new/resume/list/show/edit`는 사용자 준비 기록을 다루고, `setup apply`는 명시 승인한 설치를 수행한다. `init`과 `start`는 별도 명령이다.
- **플랫폼별 서비스 권한을 구분한다.** Linux는 root 보호 제어 영역과 인스턴스별 비root 계정의 systemd 시스템 서비스를 사용한다. Mac은 로그인한 사용자의 LaunchAgent를 사용한다.
- **불명 상태와 원자료를 보존한다.** 시작 요청·로컬 READY·네트워크 합의는 다른 판정이다. 정상 종료 근거가 모자라면 UNKNOWN을 유지하며 자동 재초기화·DB 삭제·강제 종료로 성공을 만들지 않는다.
- **실제 실행과 자동 회귀를 구분한다.** Ubuntu·macOS CI의 CLI 빌드/검사, 고정 공급물 확인, 노드·서비스 인수, 사용성 관측은 별도 근거다. Linux 노드·systemd 실물 인수와 사람 과업은 아직 미실행이다.

## 현재 작업 계획

| 문서 | 읽을 내용 |
| --- | --- |
| [NIGO 포함 Linux 공개 시험판](./2026-11-18-linux-release-channel.md) | GitHub Releases, 서명된 CLI·런타임, curl 설치, draft 인수와 공개 절차 |
| [Linux 우선 구현·인수 계획](./2026-11-16-linux-first-implementation-plan.md) | Linux 공급물·systemd·설치 UX·비공개 단일노드 시험·서버/네트워크·WSL2 후속 순서 |
| [직접 CLI 간소화 설계](./2026-10-22-cli-simplification.md) | 이름 기반 명령, 순차 준비·조회·수정, 기존 CLI/JSON과의 호환 |
| [CLI 구현 계획](./2026-10-22-integrated-operations-ux-implementation-plan.md) | CLI-01~05의 범위와 기존 UX 계획에서 이어진 기능 |
| [관리 CLI와 실행기 분리](./2026-10-23-independent-management-cli.md) | 고정 실행기 검증·위임, 관리 CLI 독립 업데이트와 노드 자산 보존 |
| [관리 CLI 배포·업데이트](./2026-10-23-management-cli-distribution.md) | CLI 전용 bundle, 사용자 설치·버전 선택/복귀·셸 연결 |

Linux 시험은 빌드를 사전 준비로 두고 설치·초기화·서비스 실행·진단·SSH 단절 유지·정상 종료·같은 DB 재시작까지 진행한다. 기술 결과와 사용 편의성은 나누어 기록한다. 전달 안내서·결과/개선 양식·개별 실행 기록은 Git 밖의 비공개 패키지로 관리하며, 이 저장소에는 재사용하는 준비 도구와 제품 설계를 보관한다.

## 구성 요소별 설계와 현재 사용법

| 영역 | 설계 근거 | 현재 사용법 |
| --- | --- | --- |
| 전체 구조·엔진 책임 경계 | [제품 아키텍처](./2026-09-16-product-architecture.md), [엔진 연동](./2026-09-16-engine-integration.md) | [문서 안내](../docs/README.md), [엔진](../docs/engine.md) |
| 설치 준비·실행·재개 | [설치 흐름](./2026-09-27-setup-install-flow.md), [제품/native 설정](./2026-09-18-product-engine-preflight.md) | [Mac 순차 준비](../docs/preparation.md), [Linux 설치·운용](../docs/linux-operations.md) |
| 등록·초기화·자료 보관 | [인스턴스 초기화](./2026-09-18-instance-initialization.md), [공급물 보관·시작 진단](./2026-10-22-managed-supply-and-start-diagnostics.md) | [인스턴스](../docs/instance.md), [Linux 설치·운용](../docs/linux-operations.md) |
| 서비스·시작 관측 | [Mac LaunchAgent](./2026-09-18-macos-launchagent.md), [시작 관측](./2026-10-19-start-observation-ux.md), [Linux 계획](./2026-11-16-linux-first-implementation-plan.md) | [인스턴스 운용](../docs/instance.md), [Linux 서비스 backend](../docs/linux-service-development.md) |
| 로그·진단·보고서 권한 | [오프라인 진단](./2026-09-21-offline-diagnostics.md), [초기화 보고서 권한](./2026-10-23-private-initialization-report.md) | [CLI](../docs/cli.md), [관리 CLI](../docs/management-cli.md) |
| JRE·패키지 공급 | [JRE 선정](./2026-10-19-jre-selection.md), [인수·릴리스 기준](./2026-09-16-acceptance-and-release.md) | [JRE 준비](../docs/runtime.md), [패키지 형식](../docs/package-format.md), [Linux 빌드·후보 준비](../docs/linux-native-build.md) |
| 네트워크 입력 | [네트워크 설정 준비](./2026-10-19-network-preparation.md) | [네트워크 가이드](../docs/network.md) |

날짜가 붙은 설계의 타입·경로·지원 범위가 현재 사용 가이드와 다르면 먼저 현재 코드와 [구현 상태](../docs/implementation-status.md)를 확인한다. 날짜별 실행 결과는 당시 artifact와 시험 조건에만 적용하며 다른 OS·엔진 후보로 승계하지 않는다.

## 엔진 후보와 인수 근거

현재 Mac 실행기 profile은 고정 RC.3·RC.4를 지원하고, Linux 개발 조합은 별도의 Linux JRE와 RC.4 lock을 사용한다. 계약 fingerprint가 같다는 이유만으로 DB migration이나 혼합 버전 네트워크를 허용하지 않는다.

- [RC.4 도입 계획](./2026-11-15-rc4-adoption-plan.md), [계약·fixture 영향](./2026-11-15-rc4-contract-fixture-impact.md), [CLI·인수 도구 설계](./2026-11-15-rc4-cli-harness-implementation.md)
- [RC.4 공급 snapshot](../contracts/nigo/engine-rc4-2026-11-15/README.md), [Mac 단일노드 결과](../results/2026-11-15-rc4-macos-service.md), [Mac 네트워크 결과](../results/2026-11-16-rc4-macos-network.md)
- [RC.3 패키지 인수 설계](./2026-10-22-rc3-package-reacceptance.md), [RC.3 공급 snapshot](../contracts/nigo/engine-rc3-2026-10-22/README.md)
- [인수 도구 안내](../tests/acceptance/README.md), [제품 JSON 계약](../contracts/bxdl/README.md)

과거 RC1·RC.2 도입 설계, Go/Rust 전환 및 Mac 우선 결정은 당시 이력으로 보존한다. 원문 공급 계약·검증 기록을 최신 구현에 맞춰 소급 수정하지 않는다.

## 보존한 UX 시안

`bxdl console`의 추가 개발은 중단했으며 직접 CLI를 현재 진입점으로 사용한다. 아래 자료는 설계 비교와 기존 동작 재현을 위한 시안이다. 현재 전체 설치·운용 흐름으로 안내하지 않는다.

- [통합 UX 설계안](./2026-10-22-integrated-operations-ux.md), [화면 명세](./2026-10-22-integrated-operations-ux-screens.md)
- [클릭 시안](./prototypes/integrated-operations-ux.html), [터미널 체험판](./prototypes/integrated-operations-ux-demo.md)
- [현재 console 제공 범위](../docs/console.md)

LaunchDaemon, 자동 재시작·재부팅 복구, 엔진/DB 업그레이드·안전 제거, Linux 다중 host·WSL2·Docker, 원격 fleet 관리와 웹 콘솔은 현재 완료 기능이 아니다. 상세 우선순위와 인수 조건은 현재 작업 계획과 지원 범위를 따른다.
