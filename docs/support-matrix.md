# 지원 후보·검증 범위

사용자 결정(2026-09-17): Rust 전환, macOS Apple Silicon에서 설치·운용 UX를 먼저 구현하고 Linux/systemd와 Docker/Compose를 후속으로 유지한다.

| 조합 | 현재 상태 |
| --- | --- |
| Rust 1.86.0 / macOS arm64 | CLI 개발·로컬 테스트·native 빌드 대상. setup·설치·cold·등록/초기화·수동 LaunchAgent·오프라인 진단 구현 |
| macOS arm64 + Java 21 + RocksDB + 사용자 LaunchAgent | macOS 26.6.2의 지정 RC.2·Temurin에서 4-validator 거래/catch-up·두 복구 경로·pin 거부 PASS. 새 V2 보관본·시작 진단은 별도 RC.2 package의 로컬13사례 PASS. OS 최소 버전·전체 G1-M 미확정 |
| RC.3 + 선정 Temurin / macOS arm64 | 전용 lock/spec 명시 선택 후 동일 서명 archive의 4-validator·두 정족수 복구·wrong-pin과 원본 공급 경로 부재 setup13사례·독립 정리 PASS. A7 NOT_RUN·전체 G1-M 미완료 |
| Linux amd64/glibc + Java 21 + RocksDB + systemd | 기존 development manifest 유지, 후속 서버 인수. Ubuntu 24.04 후보 |
| Docker/Compose | 후속 이미지·volume·network·종료/재생성 인수 필요 |
| Intel Mac·Linux ARM64·Windows·Alpine/musl·H2 제품 지원 | 별도 구현·인수 전 미지원 |

## Toolchain과 의존성

현재 설치된 Rust 1.86.0을 `rust-toolchain.toml`/Cargo의 rust-version으로 고정하고 Cargo.lock으로 전이 의존성과 checksum을 고정했다. Cargo.toml의 직접 의존성은 정확한 버전이며 lock 변경은 테스트와 함께 검토한다. fmt·Clippy·테스트가 기본 gate다. BXDL crate는 unsafe code를 금지하지만 의존 crate 내부의 unsafe 부재를 주장하지 않는다.

Rust 이식은 serde/serde_json(JSON), flate2(Rust compression backend), sha2/hex(hash), ed25519-dalek/base64(서명/PEM), cap-std(열린 directory 기준 파일 작업), rustix(OS 파일·잠금·프로세스 API), time(결과 시각)을 사용한다. Mac의 유니코드 경로 별칭 충돌 검사는 unicode-normalization 0.1.24를 사용한다. tempfile은 테스트에만 사용한다. 외부 의존성 0개라는 이전 Go 구현의 특성은 더 이상 해당하지 않는다. 실제 제품 배포 시 CLI 전이 의존성도 SBOM/NOTICE에 포함하고 별도 보안·license 검토를 해야 한다.

첫 개발 빌드는 compiler/linker 및 Cargo registry 다운로드를 필요로 한다. 고객 실행은 검증된 CLI/JRE/JAR만 소비하며 개발 도구나 registry 접근을 요구하지 않는 것이 제품 목표다. CPU/OS별 executable은 별도 빌드하며 Mac에서 Linux target type-check와 Linux linking/실행을 구분한다.

## 엔진과 Mac 실행 근거

NIGO `ROCKSDB_OPERATIONS.md`와 `tasks/2026-09-07-rocksdb-storage-implementation.md`에 macOS/Darwin arm64·Java 21의 실제 RocksDB·Node 정상 재시작 근거가 있다. `PROVIDERS.md`는 macOS aarch64 native 경로·JVM os.arch·writable/loadable temp 조건을 명시한다. 이는 BXDL의 선택한 package/JRE/launchd 조합을 인수한 결과가 아니다.

2026-10-19에 Temurin21.0.12.1+1 전체 JRE의 공급자·patch·archive hash를 선정하고 RC1과 함께 고정했다. [JRE 준비](./runtime.md)와 [NIGO J1~J3 수신 기록](../contracts/nigo/jre-feedback-2026-10-19/README.md)을 따른다. 지정 RC1/Temurin 조합의 공급자 9사례 검증을 수용했으며, BXDL signed package의 단일 노드 설치·초기화·서비스·재시작도 확인했다. 이후 [RC.1 package의 4-validator 시험](../results/2026-10-20-macos-qbft-package.md)에서 정상 거래/catch-up·pin 거부를 확인했으나 당시 정족수 복구는 INCOMPLETE였다. 2026-10-21에는 별도 RC.2 고정 package·새 DB로 [Mac 재인수](../results/2026-10-21-macos-rc2-package.md)를 수행해 4-validator 거래/catch-up·재제출 없는 실시간 복구·같은 네 DB 전체 재시작 복구·pin 거부를 통과했다. RC.1 실패 기록은 유지하며 기존 RC.1 설치의 RC.2 upgrade는 미완료다. 후속 [전체 setup 설치 UX](../results/2026-10-21-macos-rc2-setup.md)에서 지정 임시 시험 경로의20개 사례를 통과했지만, Documents 참조의 시작 전 대기 원인은 미확정이다. 원본 자동 집계 INCOMPLETE와 독립 사후 정리 PASS를 구분하며 모든 사용자 자료 경로의 지원을 주장하지 않는다. 전체 제품 NOTICE/SBOM·대응 소스 전달·최소 macOS와 G1-M 인수는 남아 있다. 과거 clean 후보·Oracle jlink 결과는 해당 revision의 이력이며 현재 공급 lock이나 제품 인수를 대신하지 않는다. REQ-0003은 OPEN이고 계약은 PROPOSED다. source commit이나 fake JAR/Java fixture를 공식 공급물로 표기하지 않는다.

2026-10-22 [RC.3 수신](../contracts/nigo/engine-rc3-2026-10-22/README.md)은 공급 archive22파일·계약/source·Console과 선정 Temurin의 실제 `engine-info`를 대조한 별도 결과다. JRE259파일 전후 불변과49모듈을 확인했다. 수신 당시에는 제품 lock을 선택하지 않았고 노드·Storage·서비스·package/A7을 실행하지 않았으며 그 snapshot은 유지한다. 이후 [RC.3 engine lock](../packaging/engine-lock.rc3-temurin-development.json)·[package spec](../packaging/package-spec.rc3-temurin.example.json)으로 새 개발 후보를 명시 선택했다. [재인수 설계](../design/2026-10-22-rc3-package-reacceptance.md)에 따른 동일 새 archive의 [4-validator·두 복구·wrong-pin](../results/2026-10-22-macos-rc3-package.md)과 [원본 공급 경로 부재 setup13사례](../results/2026-10-22-macos-rc3-setup.md)가 PASS다. 정상 종료와 독립 PID/job/LISTEN·설치 inventory 검사도 통과했다. setup은 제품 설정을 가져온 뒤 공급 자료 문답을 진행한 범위이며 전체14문답·cancel/hangup 경로를 이번에 재실행하지 않았다. RC.2와 공급자의 RC.3 검증은 각각의 이력으로 보존한다. offline Storage A7은 NOT_RUN이고 기존 설치 upgrade·전체 G1-M은 후속이다.

Manifest v1의 Mac 값 `libc=none`, `minGlibc=none`은 Linux glibc 조건이 적용되지 않는다는 뜻이다. Mac에 libc가 없다는 뜻이 아니다. Linux의 `glibc/2.34`는 기존 development 후보 계약값을 유지하며 실제 지원 보장이 아니다.

## setup 경로·파일 시스템 조건

Mac의 기본 workspace는 `$HOME/Library/Application Support/BXDL/setup`이다. 다른 개발 플랫폼은 `--workspace`를 지정한다. UTF-8·공백 경로를 지원하고 `~`·환경변수 표현을 입력 내부에서 확장하지 않는다. workspace와 자료·data·출력의 경로 관계를 보수적으로 검사하며 Mac의 대소문자·유니코드 정규화 별칭도 충돌로 다룬다. 대소문자 구분 볼륨이라도 별칭 충돌 가능성이 있는 이름을 허용한다고 보장하지 않는다.

workspace·checkpoint·출력은 symlink 조상, 의도하지 않은 hard link·특수 파일, 손상·동시 변경을 거부한다. 새 폴더 0700·파일 0600과 열린 디렉터리 기준 저장은 파일 작업 안전 조건이며 같은 UID의 engine/CLI·다른 프로세스 간 강한 격리를 만들지 않는다. ACL·mount·실제 service UID 접근 가능성과 파일 시스템별 내구성은 별도 플랫폼 인수 대상이다. 기본 설정 초안과 네트워크 준비 모드는 NIGO process·DB·키 내용을 열지 않는다. `setup --install`은 명시 확인한 설치·cold 검사·등록·init/start를 연결한다.

[등록 자료 보관과 시작 진단](../design/2026-10-22-managed-supply-and-start-diagnostics.md)의 새 V2 등록은 archive·engine lock·선택 공개키를 private `trust/`에 보관한다. 등록 완료 후에는 원본 경로 대신 보관본을 사용하지만 운용 설정·chain·키 참조·설치/control/data의 접근 조건은 그대로다. 기존 V1은 자동 변환하지 않는다. 시작 단계 표시는 마지막 진입 지점이며 접근 권한·TCC 원인의 확정이나 검사 완료 증거가 아니다. [별도 실제 RC.2 package 시험](../results/2026-10-22-macos-managed-supply.md)의 원본 경로 부재 후 재개·초기화·시작·정상 stop·재시작 13사례가 PASS다. 보호 폴더 접근 문제 전체가 해결됐다고 하지 않는다.

기본 setup 초안 모드는 서로 분리된 새 workspace를 사용하고 기존 출력에 덮어쓰지 않는다. 이 모드의 작업 폴더 내부 내보내기는 `instance.json`만 허용한다. 외부 출력 부모는 기존 폴더여야 한다. 경로 충돌이 의심되면 자동으로 다른 이름을 선택하거나 기존 자료를 이동하지 않고 명시적으로 거부한다. 실제 사용 흐름은 [setup 가이드](./setup.md)를 따른다.

## 인수 분리

이번 서비스 profile은 로그인된 현재 사용자의 GUI 세션에 명시 start로 bootstrap하는 LaunchAgent다. plist는 private control 아래 시도별로 생성하고 `~/Library/LaunchAgents`에는 등록하지 않는다. 로그인 자동 시작·자동 재시작·장기 운영 LaunchDaemon은 제공하지 않는다. Application Support 아래에서 package/control/data/config를 분리해 두는 위치를 권장하며 선택한 OS의 파일 접근·GUI 세션·실행 조건을 실제 인수해야 한다.

start는 설치본 `bin/bxdl`과 동일한 바이트의 CLI를 요구한다. 새 CLI를 기존 package에 덮어쓰는 업데이트와 기존 control/data의 migration은 아직 없다. 시도별 JAR·CLI snapshot/private 로그의 자동 정리도 후속이다. 로그아웃·OS 종료에서는 launchd가 자체 강제 종료할 수 있으므로 수동 stop의 SIGKILL 미사용을 OS 전체 보장으로 확대하지 않는다.

logs/diagnose는 저장된 최신 시도의 제한된 이벤트를 정제해 수집하는 오프라인 기능이다. 실제 health·전체 파일 무결성·DB 안전성을 검사하지 않으며 보고서 생성 성공은 서비스 인수가 아니다. 수집 시간은 파일 작업 사이에서 확인하는 예산이며 막힌 파일시스템 호출의 강제 중단을 보장하지 않는다. [진단 사용 조건](./cli.md#오프라인-기록과-지원-보고서)을 따른다.

2026-09-18 서비스 시험의 Oracle Java21.0.7 jlink는 과거 실행 fixture다. 현재 선정 Temurin과 구분하며, 그 과거 결과가 고객 재배포 승인이나 완전한 NOTICE/SBOM을 의미하지 않는다.

G1-M은 Mac 사용자 설치·launchd·동일 데이터 재시작·사용자 흐름 검증이다. 같은 사용자 권한으로 CLI와 engine가 실행되므로 Linux 전용 service UID와 같은 격리를 주장하지 않는다. 지정 RC.2와 새 RC.3 package에서 4-validator 거래·동일 DB 재시작을 확인했으며 로그인/로그아웃·sleep·실패/재개 UX와 안전한 제거 등 전체 인수는 아직 미완료다.

G1-L은 실제 Linux VM의 systemd·전용 UID·native·다중 host 네트워크 인수다. G1-D는 container 별도 인수다. 기존 사용자 node/data를 fixture로 사용하지 않는다. 개발 테스트와 과거 다른 revision의 evidence는 각 gate를 대신하지 않는다.

2026-09-18 서비스 시험은 [LaunchAgent 검증 기록](../results/2026-09-18-macos-launchagent.md)을 따른다. Documents 아래 worker 진입 전 exit 78과 임시 경로로 분리한 후의 동작 차이를 관측했지만 원인을 TCC로 확정하지 않는다. 같은 package로 기본 `Library/Application Support/BXDL` 아래의 고유 시험 경로에서도 등록·초기화·시작·상태·정지를 확인했다. 이 임시·기본 경로의 성공을 모든 사용자 경로·OS 버전의 지원으로 확대하지 않는다.

GitHub Actions는 macOS/Ubuntu의 Rust CLI fast checks를 정의한다. 각 변경의 원격 CI는 해당 PR 기록을 따르며 workflow 존재나 이전 revision의 CI를 이번 진단 변경의 PASS 근거로 사용하지 않는다. 로컬 결과도 해당 revision과 연결된 results 기록을 따른다. OS runner architecture는 실행 evidence로 기록하며 CLI CI 통과를 engine/service 인수로 확대하지 않는다.

2026-09-18 명시 초기화 시험에서는 기존 cold 전용 jlink 구성에 `jdk.management`가 빠진 점을 확인했다. NIGO `RuntimeMonitorReader`가 `com.sun.management.OperatingSystemMXBean`을 사용하므로 해당 모듈을 포함한 별도 로컬 시험 JRE로 검증한다. Java launcher hash만으로 runtime 모듈 구성을 구별할 수 없어 instance 등록/실행 시 전체 설치 inventory를 검증한다. 이 보완은 정식 JRE 선정·고객 재배포 승인·필요 모듈 전체 확정을 대신하지 않는다. [초기화 검증 기록](../results/2026-09-18-instance-initialization.md)을 따른다.

## 2026-10-19 RC1 소비자 검증

`nigo-engine-v0.1.0-rc.1`의 source `540a89f0`·JAR `f1ea80e2…`를 원격에서 수신하고 별도 lock/snapshot으로 고정했다.
macOS26.6.2(25G83) arm64와 기존 test-only Oracle21.0.7 jlink를 포함한 외부 Ed25519 서명 package에서
전체14문답·취소/재개·등록/init/start/stop·동일 DB 재시작을 확인했다. 새 유효 TLS 시험 자료와 fresh data를 사용했다.
위 과거 후보 기록을 새 RC1 근거로 재사용하지 않았다. [실행 기록](../results/2026-10-19-engine-rc1-adoption.md)을 따른다.
이 RC1 최초 시험 당시 JRE 선정은 후속이었으며 현재 선정 상태는 아래 Temurin 기록을 따른다. 최소 OS·전체 native 공급망과 4-validator/오프라인/장기 운영 인수는 별도이며 Linux 타입 검사만으로 Linux 실행 지원을 주장하지 않는다.

## 2026-10-19 Temurin 후보

Temurin21.0.12.1+1-LTS full JRE(49실제모듈)를 후보로 고정했다. Home259파일과 원본 legal을 보존한 signed package의
setup전체14문답·init/start/stop·동일 RocksDB 재시작은 macOS26.6.2(25G83) arm64에서 PASS다.
[정확한 package/runtime identity와 결과](../results/2026-10-19-temurin-runtime.md)를 따른다.
NIGO selected-runtime J1~J3 회귀는 수용했다. 공급자 SBOM 불일치·전체 제품 NOTICE/SBOM/source 제공과 최소 OS 지원은 미확정이다.
Mach-O launcher minOS11.0이나 공급자 플랫폼 표를 BXDL 최소 OS 지원으로 전용하지 않는다.
