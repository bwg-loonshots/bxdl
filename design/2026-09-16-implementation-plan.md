# BXDL 단계별 구현 계획

- 작성일: 2026-09-16
- 상태: 기반 구현 IN_PROGRESS. 현재 구현·부분 완료·공급 대기는 [작업 상태](../docs/implementation-status.md)에 기록하며, 아래 표는 전체 완료 기준을 유지한다.
- 갱신일: 2026-09-17. 사용자 확정: **Rust 전환 + macOS Apple Silicon 로컬 운영 우선**, Linux 서버/systemd와 Docker/Compose는 후속이다.
- 최신 결정: [Rust 전환·macOS 우선 UX](./2026-09-17-rust-macos-first.md). 기존 M/BX 식별자는 작업 추적을 위해 유지하며 과거 Go 실행 근거를 Rust 결과로 바꾸지 않는다.
- 연결: [아키텍처](./2026-09-16-product-architecture.md), [엔진 계약](./2026-09-16-engine-integration.md), [인수 계획](./2026-09-16-acceptance-and-release.md).

## 1. 첫 구현 범위와 단계

첫 제품은 **macOS arm64 오프라인 패키지 + Rust CLI/setup + 사용자 LaunchAgent + 기존 엔진 콘솔**이다. 하나의 validator instance를 설치·초기화·시작·종료·진단하고 동일 데이터로 재시작하는 로컬 UX를 먼저 검증한다. test-owned 4-validator mTLS 로컬 회귀를 포함하며 Linux 다중 호스트 인수와 구분한다. setup·파일 설치·등록/초기화·수동 LaunchAgent 연결까지 구현했으며 부분 검증과 전체 잔여 인수는 [현재 상태](../docs/implementation-status.md)를 따른다.

단계는 작업 분해를 위한 것이다. 매 단계마다 사용자에게 새 승인을 요구하는 절차가 아니다. 실행 권한·외부 게시가 필요한 작업은 실제 구현 당시 사용자 요청 범위와 환경 권한에 따른다. 이 문서는 구현·외부 게시를 이미 수행했다는 기록이 아니다.

| 단계 | 결과 | 선행 | 실제 완료 판정 |
| --- | --- | --- | --- |
| M0 | Mac 우선 지원 후보·Rust 도구·계약을 고정 | 없음 | macOS/arm64/JRE 후보, fixture 규칙, 격리된 Mac 테스트 profile 확보 |
| M1 | 검증 가능한 package와 CLI 기반 | M0 | fixture 기반 검증 + 실제 artifact 공급 시 source 없는 조립 |
| M2 | 고객 설정·명시 초기화·cold 검사 | M0, NIGO-02/03 | 실제 엔진 부정 사례·무변경 검사 |
| M3 | Mac setup·설치·launchd·상태·진단 | M1+M2, NIGO-04 | 사용자 profile lifecycle와 로그인·sleep·단절/실패 검증 |
| M4 | Mac package의 로컬 QBFT 인수·첫 RC | M3, NIGO-01~04/06 | G1-M의 필수 항목 모두 PASS |
| M5 | 검증한 버전 쌍 업데이트·오프라인 검사 | 해당 profile의 M4, compatibility, NIGO-05 | 해당 G1 profile 회귀 + G2 |
| M6 | Linux·Docker profile 및 고객 운영 확대 | M4 및 선택 기능의 선행 계약 | G1-L·G1-D, 원격 보안·키 운영·복원·SLO별 독립 인수 |

M1의 artifact/installer 개발, M2의 설정 adapter, M3의 setup/launchd test harness는 fixture로 병행 가능하다. 실제 init/preflight/start 성공을 주장하려면 합의된 엔진 산출물을 연결해야 한다. 공급 계약이 없으면 해당 작업만 BLOCKED로 기록하고 독립 작업을 계속한다.

G1-M은 같은 Mac UID의 사용자 profile, G1-L은 root 제어자·전용 service UID·systemd 서버 profile, G1-D는 후속 container profile이다. Linux와 Docker의 플랫폼 확장은 M6에서 추적하며 M5의 두 번째 release 공급을 기다릴 필요는 없다. 최신 결정의 R0~R6은 플랫폼별 진행 순서이고 M0~M6은 기존 작업 분류다.

## 2. M0 — 구현 기준 확정

| 작업 ID | 담당·산출물 | 내용 | 완료 기준 |
| --- | --- | --- | --- |
| BX-001 | BXDL / `docs/support-matrix.md` | macOS 최소 버전·arm64·Java 21·RocksDB·launchd 후보와 자원·디스크 조건 선정 | 후보·검증·지원 구분. Mac exact package의 native load·종료·로그인/sleep 시험 가능; Linux 조합은 별도 추가 |
| BX-002 | BXDL / `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml` | Rust 1.86.0·target·fmt/clippy·의존성 lock 고정, 기존 JSON/exit 계약 이식 | Mac native CLI·기존 부정 사례·Go archive 호환 검증; 고객 Rust/Cargo 설치 불필요 |
| BX-003 | BXDL / `packaging/runtime.lock.json` 제안 | Java 21 공급자·patch·hash·native 호환·재배포 자료 선정 | runtime 다운로드·검증·NOTICE 경로와 보안 갱신 owner 확정 |
| BX-004 | BXDL 제안, NIGO 합의 / 계약 요청 초안 | NIGO-01~06의 우선 결과·fixture·인수 기준 협의 | 실제 NIGO 요구 ID·owner·제공 상태 기록. 작성만으로 ACCEPTED 처리하지 않음 |
| BX-005 | BXDL / 테스트 환경 사양 | test-owned Mac instance/LaunchAgent·data/ports/PKI, 오프라인 구간·증거 위치 정의 | 기존 사용자 node·서비스를 사용하지 않는 teardown와 자원 상한; Linux VM·Docker는 별도 환경 |

Rust와 Mac 우선은 사용자 결정이다. Java 21 동봉·RocksDB·signed development tar.gz를 초기 안으로 유지하되 JRE 공급자·patch·hash와 macOS 최소 버전은 실제 조합으로 확정한다. Rust 의존성의 license·갱신 책임을 기록하며 Go의 무외부의존성 특성을 재사용하지 않는다. Linux 배포판·systemd/glibc와 Docker base/image 조합은 해당 후속 profile에서 선정한다.

## 3. M1 — 공급 산출물과 package 기반

| 작업 ID | 주요 파일/영역 | 구현 내용 | 선행·완료 기준 |
| --- | --- | --- | --- |
| BX-010 | `src/main.rs`, `src/cli.rs`, `contracts/bxdl` | command routing, 결과 envelope, exit code, stdout/stderr, version | BX-002. JSON schema fixture·사람용 출력·오류 출력 검증 |
| BX-011 | `src/artifact`, `engine.lock.json` | NIGO artifact/manifest hash, 계약 revision, platform 검증·cache | NIGO-01 fixture로 개발; real artifact gate는 공급 후. 변조/미지원/누락 거부 |
| BX-012 | `packaging/` | 고정 CLI/JAR/JRE·문서·대상 서비스 정의·NOTICE/SBOM만 archive 조립 | BX-003/011. source·credential·test DB 유출 없음, 반복 조립 입력/출력 기록 |
| BX-013 | `src/artifact` | `package verify`, 안전한 extract·staging, 용량/파일 수 상한 | traversal·절대경로·link escape·중복 경로·손상 archive 거부. 기존 release 미변경 |
| BX-014 | release manifest / trust policy | 모든 payload hash, 제품/엔진 식별, 지원·검증 matrix, 서명/출처 정책 | checksum과 출처 인증 구분; 공식 채널·trust key·오프라인 검증 방식 명시 |

공식 NIGO release 미제공 시 `tests/fixtures`의 가짜 artifact 또는 출처를 기록한 개발 후보만 사용한다. `engine.lock.json`에 존재하지 않는 tag/hash를 채우거나 fixture를 공식 패키지로 승격하지 않는다. root manifest와 내부 파일 hash의 범위는 canonical payload 목록으로 정의하여 자기 자신의 hash를 순환 참조하지 않게 한다.

## 4. M2 — 설정·초기화·preflight

| 작업 ID | 주요 파일/영역 | 구현 내용 | 선행·완료 기준 |
| --- | --- | --- | --- |
| BX-020 | `contracts/bxdl`, `src/config` | instance schema, network 공통/노드별/secret 참조 분리, 경로 정규화 | NIGO-02 fixture. 필수값 생략은 오류; devnet fallback 없음 |
| BX-021 | `src/config`, `config/examples` | engine YAML rendering, 공개 chain 자료 참조, 고객/테스트 예시 | mapping snapshot·schema 검사. 영속 backend·mTLS·H2 console OFF·HTTP loopback |
| BX-022 | `src/engine`, `src/instance` | 명시 로컬 data init, identity manifest, 부분 초기화 journal | 실제 NIGO-02 필요. 기존/불명 DB init 거부, 중단 후 자동 삭제 없음 |
| BX-023 | `src/engine`, `src/config` | host 정적 검사 + engine cold preflight, CHECKED/NOT_CHECKED 구분 | 실제 NIGO-03 필요. DB/WAL/config/key 무변경·listen/sign/송신 없음 |
| BX-024 | `contracts/nigo`, engine adapter | 실제 DTO·schema·fixture 출처와 version 확인 | NIGO-01~04. unknown enum·nullable·큰 정수·stale instance 처리 |

새로운 customer PKI 발급기는 만들지 않는다. 첫 버전은 운영자가 준비한 signer/TLS material과 password-file을 받아 검증하는 범위다. test harness만 격리된 CA·키를 생성하며 고객 패키지에 포함하지 않는다. key 생성/rotation/HSM은 후속 계약이다.

`init`의 내부 방식이 init 전용 엔진 tool인지 일회성 기동인지는 NIGO-02에서 확정한다. 일반 bootRun·자동 genesis를 추측 호출하여 완료하지 않는다. 초기화가 미완료인 instance의 `start`는 거부한다.

## 5. M3 — Mac setup·설치와 운영 CLI

| 작업 ID | 주요 파일/영역 | 구현 내용 | 선행·완료 기준 |
| --- | --- | --- | --- |
| BX-030 | `src/instance`, installer·setup | 사용자 범위 release·instance·권한·LaunchAgent 등록, 초안·원자 staging·journal | BX-012/013/020. 입력 취소·재실행·충돌·중단 복구, 기존 config/data/key 보존, 자동 init/start 없음 |
| BX-031 | `deploy/launchd`, `src/service`; 후속 `deploy/systemd` | Mac JVM lifecycle owner·구조화 상태, 공통 최소 adapter | Mac native/temp·로그인·sleep·SIGTERM·timeout·잔존 process 검증; Linux 전용 UID/systemd는 G1-L |
| BX-032 | `src/service`, `src/engine` | start/stop/status, role별 readiness, 서비스·engine instance 결속 | BX-022~024/030/031. 실제 restart·경로 오타·중복 start·API 단절 검사; InvocationID는 Linux adapter 내부 |
| BX-033 | `src/diagnostics`, journal | logs/diagnose, actor·operation 기록, 제한·정제·partial 보고 | 엔진 down에서도 사용 가능. secret canary·수집 예산·partial 검증; 강한 감사/UID 격리 주장 없음 |
| BX-034 | installer / 운영 문서 | uninstall, 서비스 등록 제거와 config/data/secret 보존 | 정지 불명·다른 instance·공유 release 사용 중 삭제 거부 |

Mac 설치는 자동 시작하지 않는다. setup은 설정 초안과 검사·수정·취소/재개를 제공하고 init/start는 명시 작업으로 분리한다. 수동 init·start·상태 검증 후 로그인 시 시작을 opt-in할 수 있게 한다. 사용자 LaunchAgent는 로그인 세션 실행이며 부팅 전 server daemon이 아니다. 미완료 upgrade/init·복구 필요 marker에서는 자동 시작하지 않는다. 무제한 KeepAlive는 기본으로 두지 않는다.

서비스 직접 시작에도 artifact/identity/복구 필요 gate를 적용한다. Mac은 gate 후 exec하는 제한된 wrapper 등으로 JVM에 신호가 도달하게 한다. Linux 후속 profile은 `systemctl start`와 재부팅에도 `ExecStartPre` 또는 동등한 통제를 적용한다. 실제 엔진의 startup 검증은 계속 필요하다. 서비스 제거 요청만으로 정상 종료를 확정하지 않으며 OS가 강제 종료할 수 있는 조건도 해당 profile에서 검증한다.

Mac 첫 profile은 sudo 없이 같은 사용자 UID의 CLI·engine로 운영한다. 별도 control 디렉터리·0700/0600은 같은 UID engine의 journal 변조나 instance 간 접근을 막는 강한 경계가 아니다. Linux G1-L에서는 root 제어 journal과 instance별 서비스 UID를 분리해 그 경계를 실제 검사한다. 범용 root daemon·원격 shell·무제한 passwordless sudo를 만들지 않는다.

## 6. M4 — 패키지 인수와 첫 RC

| 작업 ID | 산출물 | 구현·검증 | 완료 기준 |
| --- | --- | --- | --- |
| BX-040 | `tests/acceptance`, Mac runner; 후속 Linux/Docker runner | 완성 package의 설치·setup·lifecycle·실패 suite | 먼저 [G1-M 인수](./2026-09-16-acceptance-and-release.md), G1-L/D는 별도 profile evidence |
| BX-041 | 4-validator test harness | 같은 archive의 Mac 로컬 instance, mTLS·거래·재시작·peer 부정 사례 | finalized hash/root와 후속 확정; Linux 다중 호스트는 G1-L 별도 인수 |
| BX-042 | `docs/` 운영 문서 | 설치·PKI 입력·init·start/stop·diagnose·용량 관리·장애 절차 | 작성자가 아닌 세션/검토자가 package+문서만으로 재현 |
| BX-043 | `results/<candidate>/` | 제품/엔진/runtime hash, 환경, fixture·실행 근거, 제한 목록 | evidence sanitizer 통과; 테스트한 바이트 그대로 RC로 승격 |
| BX-044 | `.github/workflows` 또는 동등 runner | Rust PR 검사, Mac candidate 인수, 후속 Linux/Docker 인수와 release promotion 분리 | 해당 profile 필수 gate 미완료면 발행 차단; NIGO workflow·스케줄은 변경하지 않음 |

첫 RC는 **G1-M Mac 로컬 운영 패키지**다. Linux/systemd·다중 호스트·Docker·일반 고객 production·원격 API 승인을 포함하지 않는다. 동일 validator key로 standby 복제본을 자동 기동하는 failover는 제공하지 않는다.

## 7. M5 — 업데이트와 오프라인 도구

| 작업 ID | 산출물 | 내용 | 완료 기준 |
| --- | --- | --- | --- |
| BX-050 | `upgrade plan/apply`, compatibility matrix | source→target 조합 검증, stage·정지 확인·pointer 교체·기동·journal | 명시 old→new pair 거래/재시작 인수; 모든 중단점 복구; 자동 downgrade 없음 |
| BX-051 | 공식 tool adapter·문서 | NIGO inspect를 별도 JVM으로 실행, offline 독점·JSON/exit code | NIGO-05 실제 artifact 필요. Node와 동시 실행 거부, inspect의 쓰기 가능성 고지 |
| BX-052 | 제한 repair workflow | 대상·영향·명시 확인·postCommitAudit·결과 불명 처리 | 엔진 repair 범위만 제공; 전체 DB/백업 복구로 표현하지 않음 |
| BX-053 | G2 인수 report | 업데이트·도구 추가 후 해당 G1-M/L/D profile 회귀 | 선택 backend/플랫폼과 검증된 버전 쌍만 지원표에 추가 |

첫 RC에 두 번째 호환 엔진 release가 없으면 upgrade는 plan/실패 fixture까지만 제공하거나 명령을 미지원으로 둔다. 성공 경로를 시험하지 않은 apply를 공식 기능으로 내보내지 않는다. 유지보수 tool 제공 지연은 G1을 막지 않지만 해당 기능 출시는 막는다.

## 8. M6 — 후속 제품화

| 항목 | 선행 조건 | 범위 |
| --- | --- | --- |
| Linux 서버 profile | G1-M 공통 기능, Linux exact payload와 native·전용 UID·systemd adapter | BX-001/005/030~044를 Linux profile로 추가 수행하여 G1-L 판정 |
| Docker/Compose profile | 공통 engine 계약, 대상 Linux image·volume·signal/network 설계 | BX-001/005/030~044를 container profile로 추가 수행하여 G1-D 판정; archive 서명과 image 검증 분리 |
| 원격 관리·SSO/RBAC·감사 | 별도 인증된 관리 경로, engine actor/job 연결, raw RPC 우회 차단 | 이때 management service·웹 UI 필요성 평가 |
| 외부 업무 RPC | client 인증·method 정책·제한·감사, 업무 서명 권한 구분 | 운영자 관리 권한과 별도 설계 |
| 인증서 순차 교체 | trust/pin overlap, quorum 유지·중단 기준 | mTLS 교체 우선; validator signing key 교체와 구분 |
| 백업·복원 | backend별 일관된 backup primitive, identity·WAL 보존, 격리 복원·중복 signer 방지 | 복원 drill 및 RPO/RTO 실측 |
| fleet/추가 배포 형식 | 단일-node 기능 안정, multi-node orchestration·권한 모델 | Kubernetes/Helm 등은 별도 요구 발생 시; Docker는 위 후속 profile에서 추적 |
| 자동 GC 연계 | NIGO 자동 GC 안전성·진전 계약 | BXDL은 설정·관측·작업을 연결; 삭제 알고리즘 재구현 없음 |

## 9. 추천 구현 묶음과 작업 추적

1. **기반 묶음:** BX-001~005, BX-010~014. Rust CLI·manifest·package 검증·Mac harness 골격을 함께 완성한다.
2. **실행 묶음:** BX-020~024, BX-030~034. setup 초안과 실제 init/preflight/DTO 공급을 맞춰 구성→launchd→진단을 완결한다.
3. **인수 묶음:** BX-040~044. 동일 Mac artifact의 사용자 UX·로컬 4-validator·매뉴얼 재현을 완료해 G1-M RC를 만든다.
4. **유지보수 묶음:** BX-050~053. 두 엔진 release와 정식 도구 공급 후 확장한다.

PR은 위 묶음 안에서 리뷰 가능한 크기로 나누되 public DTO 변경과 소비 adapter/fixture를 같은 인수 단위로 묶는다. 무관한 engine source 이동·브랜딩 변경·schema 변경을 섞지 않는다.

각 BX 작업 기록은 `상태(PLAN/IN_PROGRESS/BLOCKED/DONE)`, `담당`, `변경 revision`, `소비 engine/hash`, `인수 ID`, `결과 링크`, `잔여 제한`을 갖는다. BLOCKED에는 필요한 공급물과 다음 책임자를 적는다. 현재 담당은 BXDL 패키지 구현 세션이며 NIGO 제공 owner의 범위 수용과 clean 개발 후보 공급을 확인했다. 정식 공급·후속 유지보수 계약의 일정은 별도로 확인한다. 작업별 최신 상태·근거·제약은 [구현 상태](../docs/implementation-status.md)를 따른다.

## 10. 일정 산정과 주요 위험

현재 NIGO 공급 일정·지원 Mac 조합·후속 Linux/Docker 환경·구현 인력이 정해지지 않아 완료 날짜를 약속하지 않는다. M0 후 각 작업을 구현/실제 인수/공급 대기 시간으로 나눠 산정한다. CPU·JRE·backend를 늘릴 때 지원 matrix와 테스트 비용을 함께 늘린다.

| 위험 | 대응·판정 |
| --- | --- |
| 공식 artifact/명시 init/cold 검사 지연 | fixture 작업 병행, 실제 gate는 열지 않음; BX-004에 dependency 기록 |
| Mac/Linux native·temp·runtime 부적합 | profile별 M0 후보 확인·M4 exact package 검증; Mac 성공을 Linux/musl/다른 CPU 근거로 사용하지 않음 |
| 같은 Mac UID의 제어 기록 변조 | 로컬 사용자 신뢰 한계를 명시; Linux 전용 UID/root 경계나 강한 격리 완료로 표시하지 않음 |
| stop/update 도중 결과 불명 | 정지·복구 필요 상태 유지; WAL 삭제·자동 restart/downgrade 없음 |
| 범위 확대 | G1에서 신규 웹·fleet·전 backend 지원 제외; 필요한 기능은 M5/M6에 독립 조건 추가 |
| 오래된 운영 문구 재사용 | 최신 source·해당 task evidence 교차 확인. 수동 GC/자동 GC·inspect/backup 차이를 유지 |

첫 구현 착수 지점은 BX-001~005와 BX-010~014다. 그 결과가 준비되면 파일·CLI·fixture가 실제로 연결된 상태에서 다음 묶음을 진행한다.
