# 지원 후보·검증 범위

사용자 결정(2026-09-17): Rust 전환, macOS Apple Silicon에서 설치·운용 UX를 먼저 구현하고 Linux/systemd와 Docker/Compose를 후속으로 유지한다.

| 조합 | 현재 상태 |
| --- | --- |
| Rust 1.86.0 / macOS arm64 | CLI 개발·로컬 테스트·native 빌드 대상. R1 setup·새 폴더 패키지 설치·개발 엔진 cold adapter. launchd 인수는 별도 |
| macOS arm64 + Java 21 + RocksDB + launchd | 첫 실제 package/UX 인수 목표. 동봉 JRE·OS 최소 버전·서비스 인수 전 |
| Linux amd64/glibc + Java 21 + RocksDB + systemd | 기존 development manifest 유지, 후속 서버 인수. Ubuntu 24.04 후보 |
| Docker/Compose | 후속 이미지·volume·network·종료/재생성 인수 필요 |
| Intel Mac·Linux ARM64·Windows·Alpine/musl·H2 제품 지원 | 별도 구현·인수 전 미지원 |

## Toolchain과 의존성

현재 설치된 Rust 1.86.0을 `rust-toolchain.toml`/Cargo의 rust-version으로 고정하고 Cargo.lock으로 전이 의존성과 checksum을 고정했다. Cargo.toml의 직접 의존성은 정확한 버전이며 lock 변경은 테스트와 함께 검토한다. fmt·Clippy·테스트가 기본 gate다. BXDL crate는 unsafe code를 금지하지만 의존 crate 내부의 unsafe 부재를 주장하지 않는다.

Rust 이식은 serde/serde_json(JSON), flate2(Rust compression backend), sha2/hex(hash), ed25519-dalek/base64(서명/PEM), cap-std(열린 directory 기준 staging·setup 저장), time(결과 시각)을 사용한다. 이번 setup에는 Mac의 유니코드 경로 별칭 충돌 검사를 위해 unicode-normalization 0.1.24를 추가했다. tempfile은 테스트에만 사용한다. 외부 의존성 0개라는 이전 Go 구현의 특성은 더 이상 해당하지 않는다. 실제 제품 배포 시 CLI 전이 의존성도 SBOM/NOTICE에 포함하고 별도 보안·license 검토를 해야 한다.

첫 개발 빌드는 compiler/linker 및 Cargo registry 다운로드를 필요로 한다. 고객 실행은 검증된 CLI/JRE/JAR만 소비하며 개발 도구나 registry 접근을 요구하지 않는 것이 제품 목표다. CPU/OS별 executable은 별도 빌드하며 Mac에서 Linux target type-check와 Linux linking/실행을 구분한다.

## 엔진과 Mac 실행 근거

NIGO `ROCKSDB_OPERATIONS.md`와 `tasks/2026-09-07-rocksdb-storage-implementation.md`에 macOS/Darwin arm64·Java 21의 실제 RocksDB·Node 정상 재시작 근거가 있다. `PROVIDERS.md`는 macOS aarch64 native 경로·JVM os.arch·writable/loadable temp 조건을 명시한다. 이는 BXDL의 선택한 package/JRE/launchd 조합을 인수한 결과가 아니다.

동봉 Java 21의 공급자·patch·archive hash·NOTICE와 최소 macOS 버전은 미선정이다. 개발 Mac의 Java를 재배포 대상으로 자동 채택하지 않는다. NIGO가 범위·안전 의미를 수용했지만 개발 후보 JAR/manifest·PROPOSED 계약/fixture는 수신했고 hash 고정 cold adapter를 구현했다. clean 후보는 #119 인계로 수신·검증했고, 공식 JRE/배포 lock·전체 engine 인수는 남아 있다. REQ-0002는 `OPEN`이고 [소비자 회신](../design/2026-09-17-nigo-feedback-response.md)에 owner·수신 revision·대기 항목을 기록한다. source commit이나 fake JAR/Java fixture를 공식 공급물로 표기하지 않는다.

Manifest v1의 Mac 값 `libc=none`, `minGlibc=none`은 Linux glibc 조건이 적용되지 않는다는 뜻이다. Mac에 libc가 없다는 뜻이 아니다. Linux의 `glibc/2.34`는 기존 development 후보 계약값을 유지하며 실제 지원 보장이 아니다.

## setup 경로·파일 시스템 조건

Mac의 기본 workspace는 `$HOME/Library/Application Support/BXDL/setup`이다. 다른 개발 플랫폼은 `--workspace`를 지정한다. UTF-8·공백 경로를 지원하고 `~`·환경변수 표현을 입력 내부에서 확장하지 않는다. workspace와 자료·data·출력의 경로 관계를 보수적으로 검사하며 Mac의 대소문자·유니코드 정규화 별칭도 충돌로 다룬다. 대소문자 구분 볼륨이라도 별칭 충돌 가능성이 있는 이름을 허용한다고 보장하지 않는다.

workspace·checkpoint·출력은 symlink 조상, 의도하지 않은 hard link·특수 파일, 손상·동시 변경을 거부한다. 새 폴더 0700·파일 0600과 열린 디렉터리 기준 저장은 파일 작업 안전 조건이며 같은 UID의 engine/CLI·다른 프로세스 간 강한 격리를 만들지 않는다. ACL·mount·실제 service UID 접근 가능성과 파일 시스템별 내구성은 별도 플랫폼 인수 대상이다. 현재 setup은 NIGO process·DB·키 내용을 열지 않는다.

서로 분리된 새 workspace를 사용하고 기존 출력에 덮어쓰지 않는다. 작업 폴더 내부의 내보내기는 `instance.json`만 허용한다. 외부 출력 부모는 기존 폴더여야 한다. 경로 충돌이 의심되면 자동으로 다른 이름을 선택하거나 기존 자료를 이동하지 않고 명시적으로 거부한다. 실제 사용 흐름은 [setup 가이드](./setup.md)를 따른다.

## 인수 분리

이번 로컬 시험용 JRE는 개발 Mac의 Oracle Java21.0.7에서 jlink로 만든 실행 fixture다. 고객용 JRE 선정·재배포 승인 또는 완전한 NOTICE/SBOM이 아니며 저장소에 동봉하지 않는다.

G1-M은 Mac 사용자 설치·launchd·동일 데이터 재시작·사용자 흐름 검증이다. 같은 사용자 권한으로 CLI와 engine가 실행되므로 Linux 전용 service UID와 같은 격리를 주장하지 않는다. 로그인/로그아웃·sleep·실패/재개 UX도 포함한다.

G1-L은 실제 Linux VM의 systemd·전용 UID·native·다중 host 네트워크 인수다. G1-D는 container 별도 인수다. 기존 사용자 node/data를 fixture로 사용하지 않는다. 개발 테스트와 과거 다른 revision의 evidence는 각 gate를 대신하지 않는다.

GitHub Actions는 macOS/Ubuntu의 Rust CLI fast checks를 정의한다. 기반 PR #1의 CI 통과는 그 revision의 과거 이력이다. PR #2는 Mac·Ubuntu CI를 통과했지만, 2026-09-18 제품/native 결합 preflight 변경의 원격 CI는 아직 미실행이다. workflow 존재나 이전 CI를 이번 PASS 근거로 사용하지 않는다. 로컬 결과도 해당 revision과 연결된 results 기록을 따른다. OS runner architecture는 실행 evidence로 기록하며 CLI CI 통과를 engine/service 인수로 확대하지 않는다.

2026-09-18 명시 초기화 시험에서는 기존 cold 전용 jlink 구성에 `jdk.management`가 빠진 점을 확인했다. NIGO `RuntimeMonitorReader`가 `com.sun.management.OperatingSystemMXBean`을 사용하므로 해당 모듈을 포함한 별도 로컬 시험 JRE로 검증한다. Java launcher hash만으로 runtime 모듈 구성을 구별할 수 없어 instance 등록/실행 시 전체 설치 inventory를 검증한다. 이 보완은 정식 JRE 선정·고객 재배포 승인·필요 모듈 전체 확정을 대신하지 않는다. [초기화 검증 기록](../results/2026-09-18-instance-initialization.md)을 따른다.
