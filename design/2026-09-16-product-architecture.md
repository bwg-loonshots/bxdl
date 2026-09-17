# BXDL 제품·배포 아키텍처

- 상태: 전체 목표 설계. 일부 제품 기반은 구현했으며 [현재 CLI](../docs/cli.md)·[구현 상태](../docs/implementation-status.md)가 실제 제공 범위다. 아래 NIGO 관련 호출은 공급 합의 전 제안이다.
- 상위 문서: [설계 인덱스](./README.md). 2026-09-17 [Rust·Mac 우선 결정](./2026-09-17-rust-macos-first.md)이 플랫폼 순서와 Mac 사용자 profile을 정의한다.

## 1. 책임 경계

| 영역 | NIGO | BXDL |
| --- | --- | --- |
| 실행·합의·저장 | canonical 규칙, QBFT, DB/WAL, 프로토콜·데이터 호환성 | 검증된 실행 산출물 소비 |
| 설정·초기화 | chain/genesis/profile/key 최종 검증, 잘못된 재시작 거부 | 입력·경로·파일 권한·설정 rendering, 사용자 절차 |
| 프로세스 | 안전한 JVM startup/shutdown, 상태·오류 보고 | launchd 우선·systemd/container 후속 adapter, 설치 위치, 운영 명령·로그·진단 |
| 유지보수 | 독점 접근·GC·복구의 안전한 primitive | 작업 계획, 호출, 결과 추적, 사용자 감사 |
| 테스트 | 소스·엔진 회귀, 프로토콜 vector, 공급 계약 | exact artifact의 설치·실행·재시작 인수 |

BXDL은 `org.nigo.*`를 import하거나 DB schema를 직접 읽고 쓰지 않는다. 공개 identity의 의미·canonical hash 계산과 validator 규칙도 엔진에서 제공받는다. protocol/profile 이름을 제품 브랜딩을 위해 바꾸지 않는다.

## 2. 첫 기술 선택

| 항목 | 추천안·이유 | 확정 조건 |
| --- | --- | --- |
| CLI | Rust, 단일 실행파일. package·설정·명령 결과와 플랫폼 adapter | Rust 1.86.0/Cargo.lock 고정, 기존 계약 이식·Mac 실행 |
| 프로세스 관리 | macOS launchd 우선, Linux systemd 후속. CLI는 관리 client | 공통 작업 상태와 플랫폼 관측 분리, 각 OS 실제 시작·종료 인수 |
| Java | 검증한 Java 21 runtime 동봉, 시스템 Java와 분리 | 공급자·patch·재배포 조건·native 실행 검증 |
| 플랫폼 | macOS arm64 우선, Linux amd64/glibc·Docker 후속 | OS 최소 버전·service manager·JRE/native를 support matrix에 고정 |
| backend | RocksDB 우선, H2는 NIGO 회귀/개발 참고 | 실제 Mac packaged-JAR/native·재시작 인수 후 Linux 별도 검증 |
| 형식 | 검증 manifest를 동봉한 `.tar.gz` + 설치 CLI | root 없는 verify와 VM 설치·제거·중단 복구 검증 |
| UI | JAR에 포함된 기존 NIGO 콘솔을 명시 opt-in한 로컬 진단에 사용 | loopback 접근 유지; 외부 운영 UI는 후속 |

고객 실행에는 Rust/Cargo·Java/Node/Gradle 빌드 도구가 필요하지 않다. 필요한 Java runtime은 검증하여 동봉한다. macOS Apple Silicon이 첫 설치·운용 UX 대상이며 Linux amd64/systemd와 Docker는 후속이다. Linux ARM64·Intel Mac·Windows·Alpine/musl은 별도 인수 전 지원으로 표기하지 않는다.

CLI 외부 명령은 명시한 실행 경로와 인자 배열로 호출하고 `sh -c`에 사용자 값을 이어붙이지 않는다. 임의 service 정의나 실행 옵션을 일반 설정으로 허용하지 않는다. Rust 명령 호출은 [std::process::Command](https://doc.rust-lang.org/std/process/struct.Command.html)를 사용한다.

## 3. 소스 구조 제안

```text
bxdl/
  src/main.rs src/cli.rs      # CLI 진입점·사람용/JSON 출력·종료 code
  src/artifact/              # lock/manifest 검증, 안전한 package staging
  src/config/                # schema, 경로 정규화, 후속 engine config rendering
  src/engine/                # 후속 HTTP DTO 및 공식 tool adapter
  src/instance/              # 후속 identity, 작업 journal, operation lock
  src/service/               # 후속 launchd/systemd/container adapter
  src/diagnostics/           # 후속 allowlist·크기 제한·비밀정보 제거
  packaging/                 # archive, runtime lock, NOTICE/SBOM, build script
  deploy/                    # 후속 launchd/systemd/container 배포 정의
  config/examples/            # customer, isolated dev/test 예시 분리
  contracts/nigo/             # revision별 실제 DTO/schema/fixture
  contracts/bxdl/             # 제품 manifest/config/CLI 결과 schema
  tests/acceptance/           # Linux VM·exact package 인수
  docs/                      # 설치·운영·장애 대응
  design/                    # 이 설계와 결정 기록
  engine.lock.json
  Cargo.toml Cargo.lock rust-toolchain.toml
```

현재는 Rust `src/cli.rs`, `src/artifact`, `src/config`와 제품 계약·패키징 예시·문서·CI를 구현했다. 나머지는 해당 단계의 실제 계약·인수와 함께 추가한다. `apps/console`, 중앙 management service와 새로운 daemon은 첫 구현에 추가하지 않는다.

## 4. 배포물과 호스트 layout

배포 archive 내부는 `bin/bxdl`, `engine/nigo-node.jar`, `runtime/`, 선택 `deploy/`, `schemas/`, `docs/`, `licenses/`, `manifest.json`, 선택 `manifest.sig`로 구성한다. 개발 v1은 manifest의 파일별 hash/size/mode와 외부 신뢰 key 서명을 검증한다. 별도 `SHA256SUMS`는 현재 형식에 넣지 않는다. 고객 설정·key·DB·개발 CA·test funding을 넣지 않는다.

최초 설치자는 검증 전 archive의 CLI를 root로 실행하지 않는다. 신뢰한 기존 검증 도구 또는 별도 검증 절차로 package 출처를 먼저 확인한다. trust public key는 검증 대상 archive만을 근거로 신뢰하지 않고 별도 신뢰 경로로 제공·확인한다. 이후에 검증된 CLI로 설치한다. 이 bootstrap 절차도 오프라인에서 재현해야 한다.

Mac 사용자 설치 위치와 같은 UID의 신뢰 경계는 [Mac 우선 설계](./2026-09-17-rust-macos-first.md)를 따른다. 아래는 **후속 Linux 시스템 profile 전용** layout이다. Mac 사용자 설치에 root/전용 UID 격리를 적용한 것으로 해석하지 않는다.

| 호스트 위치(제안) | 내용 | 소유·보존 |
| --- | --- | --- |
| `/opt/bxdl/releases/<product-version>/` | CLI·JAR·runtime·manifest·문서 | root 소유, 서비스 사용자 쓰기 금지 |
| `/opt/bxdl/instances/<id>/current` | 해당 instance의 선택 release symlink | root 소유, 작업 lock 아래 원자 교체 |
| `/etc/bxdl/instances/<id>/` | 공개 chain description 참조, instance 설정, 생성한 engine YAML, identity manifest | root 쓰기, 해당 서비스 그룹 읽기; 업데이트 보존 |
| `/etc/bxdl/instances/<id>/secrets/` | keystore·password-file·TLS material | root:instance 전용 그룹, 디렉터리 0750·파일 0640; 해당 서비스만 읽기 |
| `/var/lib/bxdl/instances/<id>/` | DB·WAL·native temp | instance별 서비스 사용자 소유; 업데이트·일반 제거 시 보존 |
| `/var/lib/bxdl-control/instances/<id>/` | 제품 작업 journal·설치/교체 복구 근거 | root 소유 0700, 엔진 사용자 수정 금지 |
| `/run/bxdl-control/<id>/` | 일시적인 변경 작업 lock | root 소유, 재부팅 후 재생성; 영속 identity로 사용하지 않음 |
| journald | stdout/stderr, unit·instance로 조회 | 보존·용량 정책 설정; 감사 원장과 구분 |

서비스 사용자와 읽기 그룹은 instance마다 분리한다. secret은 root가 설치·교체하고 engine은 읽기만 한다. engine tool이 key 파일 쓰기를 요구하는 경우에는 별도 계약을 검토한다. 다른 instance 또는 일반 사용자에 읽기 권한을 넓혀 해결하지 않는다. 인스턴스 ID는 제한된 문자 집합으로 검증하고 path traversal·unit 인자 삽입을 거부한다.

고객 설정의 상대경로 기준은 설정 파일이 있는 디렉터리다. 생성하는 engine YAML과 unit의 경로는 정규화된 절대경로를 사용한다. 실행자의 cwd에 의존하지 않는다. 운영 instance에 소스 폴더를 WorkingDirectory로 지정하지 않는다.

## 5. 설정과 identity

세 가지 입력을 분리한다: (1) 네트워크 공통 공개 chain/validator description, (2) 노드별 host·role·peer endpoint·storage 설정, (3) 노드별 secret 참조. 공통 genesis/profile과 validator set은 NIGO가 정규화·검증한다. 제품 설정 schema가 합의 규칙의 두 번째 구현이 되지 않게 한다.

초기 구현은 `contracts/bxdl/instance.schema.json`에 따른 JSON을 사용하며 YAML 및 엔진 rendering은 후속이다. 원래 `instance.yaml`의 제안 필드는 `schemaVersion`, `instanceId`, `chainDescription`, `role`, `nodeId`, `storage`, `ports`, `secretRefs`, `release`다. 정확한 engine 필드 mapping은 NIGO-02 합의 후 fixture로 고정한다. 비밀값 대신 파일 참조만 허용하고 config 출력·진단에서는 해당 참조도 필요한 범위만 표시한다.

설정 생성은 개발 기본값으로 fallback하지 않는다. 고객 mode에서는 영속 backend와 경로·QBFT·mTLS를 요구하고 H2 콘솔은 끈다. RPC/monitor는 초기 loopback으로 제한한다. P2P는 명시한 interface와 peer allowlist에 한해 노출한다. 고객용 RPC 원격 접근은 별도 인증/권한 경로가 검증된 후 제공한다.

init 완료 시 engine이 증명한 genesis/profile/backend/공개 validator identity와 제품·엔진 hash를 instance manifest에 기록한다. 첫 start에서 이 값을 대조하고 runtime 관측 정보를 보강한다. 재시작 시 경로 존재 검사만으로 동일 체인을 판정하지 않고 엔진 검증 결과와 비교한다. chain ID 하나로 identity를 결정하지 않는다.

## 6. CLI와 결과 모델

아래는 **전체 BXDL 목표 명령**이며 NIGO의 현재 flag 또는 지금 실행 가능한 CLI 목록이 아니다. 현재 version은 CLI identity만, preflight는 `--config` 로컬 metadata만 제공하며 `package build`와 `config validate`를 추가했다. 실행 예제는 [현재 CLI](../docs/cli.md)를 따른다.

| 명령 | 수행 범위·권한 | 필수 조건 |
| --- | --- | --- |
| `bxdl version --json` | 제품·동봉 엔진·runtime·계약 식별, 일반 사용자 | 고정 manifest; runtime 관측과 구분 |
| `bxdl package verify <archive>` | 서명/출처·hash·manifest·platform 검사, 읽기 전용 | 검증한 trust root; DB·systemd 접근 없음 |
| `bxdl install <archive> --instance <id>` | release·디렉터리·service 등록, 설치 profile의 권한 사용 | verify 성공; 기존 내용 충돌 거부; 자동 init/start 없음 |
| `bxdl config render --input <file> --output <file>` | 설정 검증·출력, 대상 경로 쓰기 | overwrite 명시 동의, secret 원문 출력 없음 |
| `bxdl preflight --instance <id>` | 호스트 정적 검사 + NIGO cold 검사 | 쓰기 probe/DB open/listen/signing 없음; 미검사는 NOT_CHECKED |
| `bxdl init --instance <id> --confirm-new-data` | 로컬 신규 데이터 초기화, 운영 권한 | NIGO-02 init 계약·독점 접근, 기존 데이터 거부 |
| `bxdl start / stop --instance <id>` | 등록된 플랫폼 service 제어, 운영 권한 | 허용된 instance 및 engine 안전 조건 |
| `bxdl status --instance <id> --json` | 플랫폼 service와 engine 관측 결합 | stale/UNKNOWN 포함; 관측 불가를 0/false로 대체하지 않음 |
| `bxdl logs --instance <id>` | 해당 service 로그의 범위 제한 조회 | OS 로그 권한; 출력에 민감정보 경고/필터 정책 |
| `bxdl diagnose --instance <id> --output <path>` | 정제한 진단 묶음 생성 | allowlist·총량·시간 상한, engine 정지 상태에서도 가능 |
| `bxdl upgrade plan / apply ...` | M5에서 변경 계획 또는 명시 적용 | 두 release 호환성 evidence, 정지 확인·작업 journal. G1에서는 미지원 |
| `bxdl uninstall --instance <id>` | service·제품 등록 제거, 설치 profile의 권한 사용 | 실제 정지 확인; config/data/secrets 기본 보존 |

첫 Mac profile은 사용자 권한 설치, 후속 Linux 시스템 profile은 명시 관리자 권한을 사용한다. CLI가 비밀번호를 수집하거나 자동 sudo, setuid를 제공하지 않는다. 엔진 JVM은 root로 실행하지 않는다. 세밀한 원격 RBAC는 후속이다.

`init`은 기존 네트워크의 공통 chain description을 받아 로컬 data를 준비하는 작업이다. 새 네트워크 생성·validator membership 변경 명령이 아니다. 공식 NIGO 도구 또는 합의한 oneshot 방식으로 서비스 UID에서 실행한다. 해당 호출 계약이 없으면 실제 init/start 인수는 BLOCKED다.

공통 JSON envelope의 제안 필드는 `schemaVersion`, `command`, `instanceId`, `operationId`, `outcome`, `reasonCode`, `observedAt`, `checks`, `data`다. 큰 정수·hash·nullable 의미와 안정된 reasonCode를 `contracts/bxdl/`에서 정의한다. stdout은 JSON 하나, 진단 로그는 stderr로 분리한다.

제안 exit code: `0` 요청 처리 성공, `2` 인자/schema 오류, `3` artifact 불일치·미지원 조합, `4` 권한·안전 조건 거부, `5` 관측 불가·미완료 검사, `6` timeout·결과 불명, `7` 내부/의존 실행 실패. `status`의 0은 조회 성공일 뿐 노드 정상 증명이 아니다. 자동화용 `status --require-ready`는 역할별 필수 근거 충족 시에만 0, 상태 미충족은 4, 관측 불가는 5를 반환하도록 별도 정의한다.

## 7. 플랫폼 서비스와 장애 처리

첫 Mac launchd 설계와 로그인·종료·sleep 인수는 [Mac 우선 설계](./2026-09-17-rust-macos-first.md)를 따른다. 아래 systemd·root 제어 파일·cgroup 정책은 후속 Linux profile에 한정한다. 공통 journal과 사용자 결과에는 특정 service manager field를 필수로 고정하지 않는다.

systemd는 Java를 직접 `ExecStart`하고 CLI는 `systemctl`/`journalctl`의 구조화된 속성을 adapter에서 소비한다. 별도 PID 파일 기반 supervisor를 만들지 않는다. `MainPID`와 unit InvocationID·engine instance ID를 함께 추적하며 PID만으로 kill하지 않는다.

첫 unit 정책은 `Restart=no`, 종료 SIGTERM, 명시한 stop timeout, `SendSIGKILL=no`로 제안한다. timeout 시 `STOP_TIMEOUT`/결과 불명으로 남기고 release 전환·재시작·DB 도구 실행을 금지한다. systemd의 최종 unit 상태와 잔존 cgroup 프로세스를 확인해야 정지 완료다. OS 종료·전원 손실까지 정상 종료를 보장한다는 의미는 아니다.

CLI의 호출 timeout은 systemd 작업 또는 엔진 작업 취소와 다르다. 호출이 끊기면 operation journal과 실제 unit/engine 상태를 재조회한다. 자동 재시도·새 init·강제 kill을 하지 않는다. systemd 옵션은 선정 배포판의 매뉴얼과 VM 실패 시험으로 고정한다. [systemd 종료 동작 원문](https://github.com/systemd/systemd/blob/main/man/systemd.kill.xml).

init/start/stop/upgrade/uninstall은 같은 instance 변경 lock으로 직렬화한다. unresolved 작업은 새 변경을 막는다. CLI 우회 `systemctl start`에도 root가 작성한 startup-state 요약과 artifact/identity gate를 적용한다. 서비스 계정은 `/etc/bxdl/instances/<id>/startup-state.json`만 읽을 수 있고 root의 전체 제어 journal은 읽거나 수정하지 못한다. startup gate는 이미 CLI가 보유한 변경 lock을 다시 기다리지 않으며, journal 단계에 대응하는 명시적 start 허용 상태를 읽어 판단한다. 명시 enable된 정상 instance는 재부팅 시 같은 gate를 거쳐 시작할 수 있다. 재부팅 뒤 미해결 작업·결과 불명·복구 필요 상태에서는 자동 허용하지 않는다.

unit hardening은 전용 사용자, root 소유 실행파일, 제한된 쓰기 경로, 불필요한 privilege 차단부터 적용한다. JVM native library 추출용 전용 temp 경로를 지정하고 noexec·read-only 설정과 충돌하는지 실증한다. 파일 descriptor·memory·CPU·디스크 여유 기준은 측정 후 명시하며 임의 성능 SLO를 선언하지 않는다.

## 8. 상태·진단·제품 작업

상태는 `service`(설치/프로세스), `engine`(기동/실패), `chain`(identity/sync), `consensus`(role/progress), `observation`(시각/출처)로 분리한다. health 200, callback 완료, head 정체 어느 하나로 전역 quorum을 판정하지 않는다. IDLE·UNKNOWN·미지원·비활성을 보존하며 instance 재시작 전 관측은 stale 처리한다.

제품 작업 journal은 operation ID, actor UID, instance, 대상/기존 release hash, config fingerprint, 단계·시각·결과 코드만 기록한다. Linux 시스템 profile의 root 제어 기록은 engine 쓰기 경로 밖에 둔다. 첫 Mac 사용자 profile은 같은 UID이므로 별도 control 경로만으로 engine 수정 방지를 보장하지 않는다. 원문 secret·환경변수·CLI password·전체 config·원문 DB를 넣지 않는다. BXDL operation ID와 NIGO job/command ID는 별도로 연결한다. 초기 journal은 로컬 운영 기록이며 위변조 방지 기업 감사 시스템은 아니다.

진단은 package identity·정제한 설정 요약·허용된 health/progress·제한된 로그·디스크/권한 요약을 수집한다. DB나 key 파일을 열지 않고 원본 경로도 최소화한다. 예산 초과·API timeout은 추가 수집을 멈추고 partial과 누락 사유를 기록한다. 파일 I/O·하위 도구가 정해진 시간 안에 반드시 끝난다는 보장은 아니다. 자신이 만든 진단 subprocess만 회수하고 engine에 종료 신호를 보내지 않는다. 비밀정보 패턴 fixture로 유출을 검사한다.

## 9. 업데이트·제거

M5 업데이트는 VERIFY → STAGE → COMPATIBILITY_CHECK → STOP_REQUESTED → STOPPED_CONFIRMED → SWITCH_REQUESTED → SWITCHED → START_REQUESTED → VERIFIED 순서로 journal을 남긴다. source/target artifact와 DB/profile 호환성 근거가 없는 조합은 apply를 거부한다. `plan`은 읽기 전용이며 실제 변경 전에 입력 fingerprint를 다시 검사한다.

각 외부 변경 전 intent를 기록·동기화하고 완료 뒤 결과를 기록한다. pointer는 같은 filesystem의 임시 symlink+rename과 디렉터리 동기화로 교체한다. crash 후에는 journal만 신뢰하지 않고 pointer·systemd job/InvocationID·잔존 cgroup·engine identity와 대조한다. data 초기화 성공 후 manifest 기록 전 중단도 incomplete로 남겨 엔진 판정 전 재초기화하지 않는다. 파일 동기화 구현은 선정 filesystem/VM에서 검증하며 전원 손실 내구성 전체를 이 절차만으로 보장하지 않는다.

release symlink만 원자적으로 바꾸고 config/data/key/WAL은 보존한다. 신규 binary가 DB를 열었거나 그 여부가 불명이면 이전 binary로 자동 복귀하지 않는다. switch 전 실패와 switch 후 실패를 구분하여 정지 유지·현재 pointer·마지막 확정 단계를 보여준다. rollback은 별도 검증된 호환 조합에만 후속 제공한다.

동일 버전 install 재실행은 일치 확인 후 성공 또는 충돌 실패로 끝난다. 부분 설치는 소유한 journal로 복구하고 기존 instance의 파일을 추측 삭제하지 않는다. uninstall은 활성 작업·잔존 process가 있으면 거부하며 데이터 삭제 옵션은 첫 범위에서 제외한다.

## 10. 실제 소스 근거

다음은 NIGO 기준 revision에서 확인한 경로다.

- `nigo-java/nigo-node/build.gradle`: canonical JAR의 React bundle 포함.
- `nigo-java/nigo-node/src/main/resources/application.yml`: 개발 기본값.
- `nigo-java/nigo-node/src/main/java/org/nigo/node/initializer/GenesisBlockInitializer.java`: 일반 startup의 빈 DB genesis 생성.
- `nigo-java/nigo-node/src/main/java/org/nigo/node/config/MonitorConsoleWebConfiguration.java`: console loopback 제한.
- `nigo-java/nigo-node/src/main/java/org/nigo/node/service/gc/StorageGcJobService.java`: 고정된 로컬 비인증 actor.
- `nigo-java/nigo-crypto/PROVIDERS.md`, `nigo-java/nigo-node/ROCKSDB_OPERATIONS.md`: native/platform 제약.
- `nigo-e2e-test-suite/scripts/node-harness.mjs`: 소스 빌드 기반 E2E와 제품 인수의 차이.
