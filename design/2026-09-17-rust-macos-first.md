# Rust 전환과 macOS 우선 설치·운용 UX

- 결정일: 2026-09-17
- 사용자 확정: Go 기반을 Rust로 전환하고 macOS에서 설치·운용 UX를 먼저 구현한다.
- 우선순위: macOS Apple Silicon 로컬 운영 → Linux 서버/systemd → Docker/Compose. Linux는 후속 서버 배포 목표로 유지한다.
- 갱신: Rust 기반 이후 R1 setup 입력·수정·checkpoint/재개·새 제품 설정 출력까지 구현했다. R2의 새 폴더 파일 installer와 R3의 NIGO engine-info/cold 연결을 구현했다. setup의 패키지 선택·instance 등록·init/start/stop·launchd는 후속이다.
- 2026-09-18 추가: clean 후보 수신과 제품/native 결합 cold 검사를 구현했다. [후속 설계](./2026-09-18-product-engine-preflight.md)가 후보 공급·종료/동기화 대기에 관한 이 문서의 이전 상태보다 최신이다.
- 최신 상태: [구현 상태](../docs/implementation-status.md), [setup 가이드](../docs/setup.md), [CLI](../docs/cli.md). 코드 존재와 실제 검증·운영 설치 완료를 구분한다.

## 1. 결정 이유와 이전 설계 변경

운영자가 Linux 장비를 자주 사용할 수 없으므로 실제로 반복 사용할 Mac에서 초기 설정, 실패 수정, 시작·종료·재시작을 검증한다. NIGO에는 macOS/Darwin arm64·Java 21·RocksDB 정상 실행/재시작 근거가 있다. 이 근거는 새 BXDL package·선정 JRE·launchd 인수를 대신하지 않는다.

초기 Go 선택은 표준 라이브러리 중심 구현 편의였다. 장기 제품 구현 언어를 Rust로 변경하되 기존 manifest/config/CLI JSON 계약과 테스트의 안전 조건을 보존한다. 저장소에 두 언어의 제품 구현을 병행 유지하지 않는다. 2026-09-16 Go 검증 기록은 과거 evidence로 보존한다.

Rust 1.86.0과 Cargo.lock을 고정한다. 애플리케이션 crate는 unsafe code를 금지한다. JSON·압축·서명·디렉터리 접근은 테스트한 crate 조합을 사용하고 전이 의존성은 lock으로 추적한다. 정식 배포 시 license 목록과 SBOM을 추가 검증한다. 외부 의존성 0개라는 기존 Go 특성은 Rust 구현에는 적용되지 않는다.

## 2. 처음 사용할 때의 UX

최초 진입은 대화형 CLI `bxdl setup`이다. R1에서는 validator/RocksDB의 14개 설정 항목을 입력·수정하고 초안을 저장·재개하거나 새 제품 JSON으로 내보낸다. 비대화형은 `--from`으로 기존 설정을 가져오거나 `--resume`으로 초안을 이어가며 `--non-interactive`를 명시한다. JSON 결과는 비대화형에서만 제공한다. 별도 Mac 네이티브 앱·Electron/Tauri·원격 관리 서버는 도입하지 않는다.

아래 표는 전체 목표 UX다. 이번 R1은 2~4단계의 제품 설정 초안과 로컬 metadata 검사 부분만 제공한다. 독립 install과 engine cold 명령이 추가됐다. package 선택을 setup에 연결하기, 실제 instance 등록·init/start·콘솔 연결·제거는 후속이다. 후속 운영 조회는 기존 NIGO의 로컬 브라우저 콘솔을 재사용한다.

| 단계 | 운영자가 하는 일 | 제품이 보여줄 결과와 실패 처리 |
| --- | --- | --- |
| 1. 설치 자료 확인 | 패키지와 외부 신뢰 key 지정 | 제품/엔진/runtime 식별, 대상 OS/CPU, 검증 결과. 잘못된 자료면 설치 전 중단 |
| 2. 인스턴스 만들기 | 이름·저장 위치·포트 선택 | 실행파일/설정/데이터 위치 구분, 다른 instance와 충돌 원인 표시 |
| 3. 네트워크 자료 등록 | 공통 chain 자료·기존 signer/TLS 파일 선택 | 필요한 자료 목록과 누락/권한 오류. production key 자동 생성 없음 |
| 4. 사전 검사 | 결과를 보고 잘못된 항목 수정 | 통과/실패/미검사와 수정 안내. 검사를 위해 DB를 열거나 노드를 시작하지 않음 |
| 5. 명시 초기화 | 새 로컬 데이터 생성에 대한 확인 | 생성 대상과 영향 표시, 이미 데이터가 있거나 부분 초기화면 거부/복구 안내 |
| 6. 시작 | 검증된 instance 시작 | 시작 중→실제 engine 상태. process 존재만으로 준비 완료를 표시하지 않음 |
| 7. 운영 | 로컬 콘솔 열기, 상태·로그·진단 확인 | identity·관측 시각·동기화·합의 진행 분리. 미지원 action 이유 표시 |
| 8. 종료·재시작 | 종료 후 같은 instance 다시 시작 | 정상 종료 근거, 동일 key/DB/WAL·identity 대조, 결과 불명 시 다음 변경 차단 |
| 9. 제거 | 설치 등록과 실행파일 정리 | 보존할 설정/키/데이터를 먼저 표시하고 기본 보존 |

뒤로 가기·다시 실행하기는 파일 덮어쓰기나 새 genesis 생성의 동의가 아니다. 입력 답은 제품 형식 규칙을 검사한 뒤 0600 checkpoint로 저장하며 완성된 초안의 로컬 검사 실패도 보존한다. 누락된 참조 파일이 있어도 초안·설정을 저장할 수 있고 저장 성공은 엔진 준비 완료를 의미하지 않는다. init/start는 별도의 후속 명시 작업이다. `:cancel`·EOF 뒤에는 같은 workspace의 `--resume`으로 마지막 완료 저장부터 이어간다. password 원문을 인자·설정·결과에 넣지 않는다.

기본 workspace는 `$HOME/Library/Application Support/BXDL/setup`이며 새 폴더만 만든다. 이미 있는 초안에는 `--resume`이 필요하다. 새 config 출력은 기존 파일을 덮어쓰지 않고 workspace 안에서는 `instance.json`만 허용한다. 직접 입력은 최초 cwd, 가져온 참조는 원본 config 기준으로 절대화하여 내보내기·재개 때 경로 의미를 보존한다. 초안은 제품 관리 instance나 engine init journal이 아니다.

## 3. 첫 Mac 설치 profile

첫 대상은 `darwin/arm64`(Apple Silicon), Java 21, RocksDB다. 실제 지원 macOS 최소 버전과 동봉 JRE의 공급자·patch·archive hash·native 실행은 별도 선정·인수한다. Intel Mac과 universal binary를 자동 지원한다고 표시하지 않는다.

설치 위치 제안은 사용자의 `Library/Application Support/BXDL/` 아래 releases/instances/control, 로그는 `Library/Logs/BXDL/`다. 경로는 사용자 계정과 설치 profile에서 해석하며 공백을 정상 지원한다. 사용자에게 raw plist나 시스템 경로 편집을 요구하지 않는다. 기본 설치는 sudo 없이 해당 사용자 범위다.

이 profile에서 CLI와 engine는 같은 OS 사용자로 실행된다. 0700/0600과 논리 디렉터리 분리만으로 engine가 제어 기록을 수정할 수 없다고 주장하지 않는다. 같은 사용자 내 instance는 강한 보안 격리 단위가 아니다. Linux의 root 제어자/전용 service UID 경계와 구분하며, 강한 격리는 별도 설치 profile로 설계·인수한다.

Mac 압축 패키지에는 Mac CLI·Mac JRE·검증한 JAR/native 자료를 넣는다. 초기 형식은 signed development tar.gz를 유지한다. 배포용 .app/.pkg, Developer ID 서명·notarization·Gatekeeper 경험은 실제 일반 사용자 배포 단계에서 별도 결정한다. 현재 Ed25519 package 서명은 Apple code signing을 대신하지 않는다.

## 4. 공통 로직과 플랫폼 경계

| 공통 모듈 | 플랫폼 adapter |
| --- | --- |
| artifact·서명·설정·identity·작업 상태·NIGO DTO | 설치 경로·사용자/권한·host 검사 |
| install/init/start/stop/status/diagnose 사용자 계약 | launchd / systemd / container 실행·관측·로그 |
| 안전한 초기화·동일 데이터 재시작 원칙 | 실제 서비스 ID·process instance·종료 확인 |
| 시작 전 검사와 미완료 작업 차단 | 서비스 직접 시작 시 같은 검사 적용 |

`src/service/`에 구체적인 구현이 필요해질 때 공통 최소 경계를 만든다. 대규모 범용 plugin framework를 선행 구현하지 않는다. 공통 상태 모델에 systemd의 unit·InvocationID·cgroup을 필수 field로 박지 않고 adapter 내부 관측으로 둔다. engine instance ID와 제품 operation ID는 공통으로 연결한다.

Rust crate 구성은 `src/{cli,error,artifact,config,setup,install,engine}`이며 instance/service/diagnostics는 실제 다음 기능과 함께 추가한다. setup의 checkpoint·파일 출력은 공통 제품 코드이며 Mac 경로 별칭 충돌을 보수적으로 검사한다. 후속 프로세스 호출은 인자 배열을 사용하고 shell 문자열 연결이나 임의 실행 옵션을 제품 설정으로 받지 않는다.

## 5. launchd 실행·종료·재개

Mac 첫 profile은 사용자 LaunchAgent로 제안한다. 사용자가 로그인한 세션에서 실행되며 부팅 전용 server daemon과 다르다. 최초 설치는 자동 시작하지 않는다. 로그인 시 시작은 정상 수동 초기화·실행 검증 뒤 명시 옵션으로 제공한다. 기본 무제한 KeepAlive/자동 재시작은 사용하지 않는다.

기동 전 artifact·instance·미완료 작업 gate는 launchd로 직접 시작해도 적용되어야 한다. JVM을 직접 관리하고 필요 wrapper는 gate 후 exec하여 신호가 JVM까지 전달되게 한다. 단순 PID 파일로 service ownership을 추측하지 않는다.

정상 stop은 engine의 합의한 종료 계약과 process 종료를 확인한 뒤 job 등록/상태를 정리한다. launchd job 제거를 호출했다는 사실만으로 저장소 정상 종료를 표시하지 않는다. launchd/system shutdown이 강제 종료할 수 있는 조건은 해당 OS와 실제 JRE에서 별도로 시험한다. timeout·관측 단절 시 자동 kill/restart·DB 재초기화는 하지 않는다.

로그아웃·로그인, Mac 잠자기/깨우기, 터미널 종료, GUI 세션 부재, launchd 미등록, 부분 설치·기동 실패를 UX 인수에 포함한다. 노트북의 sleep 동안 네트워크 liveness를 보장하지 않는다. launchd 인수 완료 전 현재 CLI는 해당 명령을 UNSUPPORTED로 유지한다.

## 6. 단계와 완료 판정

| 순서 | 산출물 | 완료 기준 |
| --- | --- | --- |
| R0 기반 | Rust 기반과 Mac profile, 설계 갱신 | 기존 부정 사례·JSON/exit 계약 이식, 서명 round-trip·Go archive 호환 확인, Mac native 실행, fmt/clippy/tests. 과거 결과는 해당 revision evidence |
| R1 구현 | setup 입력·초안·오류/재개·설정 출력 구현 | 14개 입력·취소/EOF·checkpoint·가져오기·새 출력과 부정 사례. 실제 검사/미검사 경계를 화면·결과에 명시하고 해당 revision으로 검증; 전체 설치 완료 아님 |
| R2 일부 구현 | 검증된 Mac package를 새 폴더에 설치·완료 receipt | host/서명/hash/alias 검증·기존 경로 거부. 중단은 완료 receipt 없는 폴더로 남기고 자동 resume하지 않음. setup 연결·instance journal 후속 |
| R3 cold 구현 | pinned engine-info·명시 native config cold 검사 | 실제 개발 후보로 INCOMPLETE 의미 소비. init/start/status/stop·launchd는 QBFT 종료 증명 등 후속 계약·인수 뒤 구현 |
| R4 | G1-M Mac 로컬 운영 인수 | 동일 package로 전체 사용자 흐름, 별도 test-owned 4-validator 로컬 회귀. multi-host/Linux 운영 승인은 아님 |
| R5 | G1-L Linux 서버 인수 | systemd·전용 UID·native·재부팅·다중 host/QBFT를 Linux exact package로 검증 |
| R6 | G1-D Docker 인수 | 같은 engine 계약의 이미지·volume·신호/정지·network·재생성/재시작 검증 |

NIGO-01~04/06의 공급 계약은 Mac에서도 필요하다. 기존 source 기반 Mac 실행 근거를 공식 artifact 인수로 승격하지 않는다. Mac UX는 fixture로 병행 개발할 수 있으나 미구현 초기화/검사를 성공 처리하지 않는다.

## 7. Linux와 Docker 후속 유지

Linux는 시스템 설치·service UID 격리·systemd의 별도 profile을 유지한다. Mac의 성공은 Linux native·권한·서비스 인수를 대신하지 않는다. 기존 linux/amd64 development manifest도 Rust verifier에서 계속 받는다.

Docker는 공통 payload를 OCI 이미지로 조립하고 엔진의 실행을 container runtime이 관리한다. base/image digest·전체 이미지 공급물 검증, 영속 DB/WAL volume, 읽기 전용 secret, bind/advertise/publish 주소, readiness, SIGTERM 이후 timeout과 강제 종료를 별도 설계한다. 기존 archive 서명이 이미지 전체를 인증하지 않는다. ARM64/Linux와 musl은 별도 native 인수 전 자동 추가하지 않는다.

## 8. 요구 원장·근거

NIGO `REQ-0002`의 제공 owner는 범위를 수용했고 BXDL은 피드백 3절의 안전·인수 의미와 4·5절의 결과 교환에 동의했다. [소비자 회신](./2026-09-17-nigo-feedback-response.md)에 수신 revision·owner·상세 계약 대기를 기록한다. 개발 후보 API/flag/fixture/JAR를 수신했고 [계약 snapshot](../contracts/nigo/development-2026-09-17/README.md)을 소비한다. PROPOSED 계약과 dirty 개발 후보이며 clean 공급·QBFT 종료/drain·전체 G1-M 인수가 남아 요청 완료로 올리지 않는다. 이번 BXDL 작업은 공유 중인 NIGO 원장을 수정하지 않으며 엔진 source·사용자 node/data도 변경하지 않는다.

- NIGO `nigo-java/nigo-node/ROCKSDB_OPERATIONS.md`, `tasks/2026-09-07-rocksdb-storage-implementation.md`: macOS arm64/Java21 RocksDB 근거.
- NIGO `nigo-java/nigo-crypto/PROVIDERS.md`: native resource·temp·JVM architecture 조건.
- [Apple launchd 설명](https://developer.apple.com/library/archive/documentation/MacOSX/Conceptual/BPSystemStartup/Chapters/CreatingLaunchdJobs.html).
- [Rust ownership](https://doc.rust-lang.org/book/ch04-01-what-is-ownership.html), [Cargo lockfile](https://doc.rust-lang.org/cargo/guide/cargo-toml-vs-cargo-lock.html).
