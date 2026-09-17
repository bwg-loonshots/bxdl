# BXDL 패키지 인수·릴리스 계획

- 상태: 실행 전 계획. 아래 PASS 기준은 실제 검증 결과가 아니다.
- 갱신일: 2026-09-17. [Rust 전환·macOS 우선 결정](./2026-09-17-rust-macos-first.md)을 적용하며 기존 test ID를 보존한다.
- 연결: [구현 계획](./2026-09-16-implementation-plan.md), [엔진 계약](./2026-09-16-engine-integration.md).

## 1. 지원 matrix와 인수 환경

| 환경 | 첫 역할 | 판정 |
| --- | --- | --- |
| macOS arm64 + 지정 Java 21 + RocksDB + 사용자 LaunchAgent | 첫 로컬 운영 패키지 후보, G1-M | 최소 macOS·JRE·native 전체 조합 선정 후 실제 인수. 같은 사용자 UID의 신뢰 한계 명시 |
| Linux x86_64/glibc + 지정 Java 21 + RocksDB + systemd | 후속 서버 profile, G1-L | 배포판·glibc·systemd·JRE·전용 UID·root 제어 경계를 별도 인수 |
| Docker/Compose의 지정 Linux image·CPU·runtime | 후속 container profile, G1-D | image digest·volume·network·signal·재생성 인수; Mac host만으로 Linux arm64/musl 지원을 주장하지 않음 |
| file H2 | NIGO 공급자 회귀·개발 비교 | BXDL 첫 지원 gate 밖. 고객 지원 추가 시 별도 matrix 필요 |
| Intel Mac·Linux ARM64·기타 OS·musl | 별도 후보 | 미지원. dependency에 binary가 있다는 이유로 지원 표기하지 않음 |

G1-M은 test-owned Mac 사용자 profile·LaunchAgent·데이터로 설치와 setup UX를 검증한다. 첫 네트워크 회귀는 동일 Mac package의 격리된 4-validator 로컬 instance다. 같은 Mac UID 안의 instance는 강한 보안 격리 단위가 아니며 이 결과는 multi-host 배포 근거가 아니다.

G1-L은 systemd가 실제 service manager로 동작하는 disposable Linux VM을 사용하고 4개 VM에 한 validator씩 설치해 다중 호스트를 인수한다. 단순 container 실행은 systemd gate를 대신하지 않는다. G1-D는 선택한 container runtime에서 별도 image·영속 volume·신호 전달·네트워크를 검증한다.

harness의 PKI·key·port·data·서비스 등록·VM/container는 모두 test-owned다. 고객 키·실행 중 사용자 node를 사용하지 않는다. teardown은 run ID로 자신이 만든 자원만 확인하여 수행한다. 테스트 환경을 준비한 뒤 설치/실행 구간에는 해당 환경의 인터넷 egress를 차단하고 테스트 노드 간 P2P와 필요한 harness 접근만 허용한다. 사용자 전체 네트워크 설정을 임의 변경하지 않는다.

## 2. Gate와 지원 주장

| Gate | 완료 의미 | 필수 범위 |
| --- | --- | --- |
| G0 | mock/fixture 기반 개발 준비 | schema·출력·artifact 실패·설정·상태 adapter. 실제 엔진 동작은 미검증 |
| G1-M | 첫 Mac 사용자 profile 로컬 운영 RC | 공통 G1 case의 Mac 조건 + MAC case + NIGO-01~04/06 실제 계약 + 문서 재현 |
| G1-L | 후속 Linux 서버 profile RC | 공통 G1 case의 Linux 조건 + systemd·전용 UID/root 제어·4-host 인수 |
| G1-D | 후속 Docker/Compose profile RC | 공통 G1 case의 container 조건 + DKR case + 실제 image/runtime 인수 |
| G2 | 검증된 버전 업데이트·offline 유지보수 | 해당 G1-M/L/D profile 회귀 + UPD/TOOL + NIGO-05·호환 버전 쌍 |
| G3 | 합의한 고객 운영 범위 승인 | 해당 OS/profile의 engine release gate, 키 운영·복원 drill·보안·장기 부하·실측 운영 기준. 원격 노출 시 인증/권한/감사 포함 |

필수 항목의 NOT_CHECKED/SKIP/환경 대기는 PASS가 아니다. capability가 없는 optional 기능은 미지원으로 명시한다. 첫 RC는 G1-M이며 G2의 두 번째 engine release 대기로 막지 않는다. G1은 profile 계열 이름이며 단독 통과로 기록하지 않는다. G1-M/L/D와 G2를 production 전체 승인으로 표시하지 않는다.

## 3. G1 공통 ID와 profile별 인수

기존 ID의 목적을 유지하고 `profile`을 함께 기록한다. 같은 ID의 Mac PASS를 Linux 또는 Docker PASS로 복사하지 않는다. 아래 공통 목록은 각 profile에서 모두 수행하며 서비스·권한 조건은 해당 열의 설명을 따른다. Mac의 같은 UID 제약은 미검사 항목을 PASS로 바꾸는 예외가 아니라 별도로 명시한 지원 경계다.

| ID | 시나리오 | PASS 기준·증거 |
| --- | --- | --- |
| ART-01 | source·Gradle·Rust/Cargo·Node/npm·저장소 token·인터넷 없는 설치 | 완성 package와 동봉 runtime만으로 설치·기동. 외부 다운로드/소스 참조 없음; Docker는 사전 전달한 image 사용 |
| ART-02 | JAR/JRE/manifest 변조·누락·미지원 계약/platform | 실행 전 거부, 기존 release/config/data 불변 |
| ART-03 | archive traversal·symlink escape·중복 path·과대 압축 | staging 밖 쓰기 없음, 명시 실패, 기존 설치 영향 없음 |
| ART-04 | package provenance·NOTICE/SBOM·payload 확인 | 별도 신뢰 경로의 trust root로 bootstrap 검증, 잘못된 서명/key 거부, 모든 payload hash 일치, credential·dev data·개발 toolchain 미포함 |
| INS-01 | 신규·동일 버전 재설치·충돌·설치 중단 | 정확한 결과와 소유한 partial 복구, 고객 config/data/key 불변 |
| INS-02 | 다른 cwd·공백 포함 archive/input 경로·인스턴스 ID 공격값 | 경로 기준 일정, 위험 ID 거부, 잘못된 서비스 이름/경로 조작 없음; Mac Application Support 경로 포함 |
| CFG-01 | 새 로컬 data 명시 init, 같은 chain identity 재시작 | engine의 genesis/profile/node/backend 결과를 기록·대조 |
| CFG-02 | start의 누락/오타/다른 DB, 다른 profile/key | fresh genesis fallback 없음, 원래 DB/key 미변경 |
| CFG-03 | 기존 data init·중간에 끊긴 init | 기존 데이터 덮어쓰기·키 생성·자동 정리 없음; incomplete 상태와 판정 절차 |
| PRE-01 | 정상/잘못된 설정·key의 cold preflight | 원본 hash·크기·mtime 및 syscall/network 계측으로 DB open·write·listen·송신·signing 없음. 읽기에 따른 atime은 별도 취급 |
| PRE-02 | DB 내부 확인 불가·검사 일부 실패 | NOT_CHECKED/오류를 보존, cold PASS를 runtime readiness로 승격하지 않음 |
| LIFE-01 | RocksDB 거래 확정→정상 종료→동일 DB/key/WAL 재기동 | 동일 head/hash·identity와 후속 거래 확정; 정상 close 근거 |
| LIFE-02 | 같은 instance 중복 start·DB lock·유사 PID/다른 서비스 | 이중 writer 거부, 타 process/data에 영향 없음; launchd job/systemd unit/container identity 확인 |
| LIFE-03 | startup/stop 지연·CLI 종료·관측 단절 | 제품의 자동 강제 kill/restart 없음, operation·서비스·engine 상태 대조, incomplete 유지. OS/runtime 강제 종료는 정상 close로 오인하지 않음 |
| LIFE-04 | CLI 우회 직접 서비스 시작·세션/호스트 재시작 | 모든 profile에 startup gate 적용, unresolved init/update·복구 필요 instance 거부. Mac 명시 opt-in 로그인 시작, Linux enable 부팅, Docker 명시 재생성 각각 검증 |
| OBS-01 | validator/observer·IDLE·UNKNOWN·실패·미지원 fixture | role별 해석 일치, HTTP 200/head 정체만으로 healthy/failure 판정 없음. 실제 가능한 상태는 엔진으로 재현 |
| OBS-02 | 엔진 교체·재시작·stale 응답·identity 불일치 | build/instance를 구분하고 이전 응답을 현재 정상으로 재사용하지 않음 |
| QBFT-01 | 같은 package의 4-validator mTLS, 단일 ingress 거래 | 같은 finalized height/hash/root, 거래 확정. Mac 로컬·Linux 4-host·container topology를 evidence에 구분 |
| QBFT-02 | 한 validator 정상 stop/start·catch-up | 같은 DB/key/WAL 재합류 후 신규 거래 확정. 별도 2/4 동작 구간에서 신규 finality 없음 |
| QBFT-03 | wrong pin/role/validator·만료 인증서·허용되지 않은 peer | 인증/identity 거부 코드, 정상 peer와 원장 안전성 유지 |
| SEC-01 | 기본 endpoint·사용자·secret·control 파일 권한 | 공통 HTTP loopback·H2 console OFF·비root JVM. Mac은 다른 일반 UID 접근 차단 및 같은 UID 격리 부재 명시. Linux는 전용 UID 간 secret 격리·engine의 root journal 변조 거부. Docker는 사용자·mount·control 분리 조건 별도 검증 |
| SEC-02 | 로그/config/API fixture에 secret canary·과대 응답 | diagnose export에 비밀 원문 없음, 예산 초과 partial·누락 사유, Node down에서도 생성 |
| PLAT-01 | 선정 OS/CPU/JRE의 crypto native·RocksDB JNI·temp | 각 exact package/image의 실제 native load·영속 재시작. unsupported/noexec 조건은 구분해 실패 |
| REM-01 | uninstall·사용 중 shared release·정지 불명 | 서비스 등록 제거는 정지 확인 뒤 수행, config/data/secrets 보존, 다른 instance release 삭제 없음 |
| DOC-01 | 다른 검토자의 package+문서만을 이용한 재현 | 설치·PKI 입력·init·start/status/stop·diagnose·restart를 추가 소스 도움 없이 완료 |

기존 E2E 시나리오의 의미를 재사용할 수 있지만 NIGO managed runner의 fresh Gradle build를 호출하지 않는다. 테스트 입력은 시작부터 끝까지 같은 archive hash와 내부 JAR hash다. test fixture의 key 생성기는 고객 설치 경로와 분리한다.

### 3.1 G1-M Mac 추가 인수

| ID | 시나리오 | PASS 기준 |
| --- | --- | --- |
| MAC-01 | setup 입력·수정·취소·터미널 종료·재개 | 저장된 초안/미완료 단계와 수정 방법 표시, 취소를 overwrite/init/start 승인으로 간주하지 않음 |
| MAC-02 | 로그아웃/로그인·sleep/wake·네트워크 단절 | 이전 관측 stale 처리와 실제 engine 재확인, sleep 중 liveness 보장 없음, 자동 재초기화/강제 재시작 없음 |
| MAC-03 | LaunchAgent 미등록·GUI 세션 부재·직접 시작 | 사용자 세션 범위와 불가 이유 표시, gate 우회 없음, job 제거만으로 정상 종료 판정하지 않음 |
| MAC-04 | 일반 사용자 설치·코드 서명 경계 | sudo 없는 사용자 위치·공백 경로 재현, 동일 UID의 secret/control 접근 한계 명시, Ed25519 검증과 Apple 서명/notarization 구분 |

### 3.2 G1-L·G1-D 후속 인수

G1-L은 기존 Linux 조건을 유지한다. LIFE-03에서 unit/InvocationID·cgroup을 대조하고 LIFE-04에서 `systemctl start`·재부팅 gate를 시험한다. SEC-01은 root 제어 journal·전용 service UID로 engine의 제어 기록 변조를 거부하며 QBFT-01~03은 4개 Linux VM으로 재현한다.

| ID | G1-D 시나리오 | PASS 기준 |
| --- | --- | --- |
| DKR-01 | image 공급물·digest·volume·secret mount | 전체 image 검증, 기존 DB/WAL 보존, 읽기 전용 secret과 비root engine, archive 서명을 image 전체 인증으로 사용하지 않음 |
| DKR-02 | SIGTERM·timeout·runtime 강제 종료·재생성 | 신호 전달과 engine close 근거 확인, 강제 종료/불명을 보존, 같은 volume/key/identity로 재시작, 자동 fresh fallback 없음 |
| DKR-03 | bind/advertise/publish·readiness·4-validator network | 노출 정책과 peer 연결 구분, 준비 상태 실증, 중복 signer/volume writer 거부, runtime topology 명시 |

## 4. G2 인수 목록

| ID | 시나리오 | PASS 기준 |
| --- | --- | --- |
| UPD-01 | 미지원 source→target, hash/manifest 불일치, stage 실패 | 현재 release와 고객 상태 보존, apply 거부 |
| UPD-02 | 지정 old→new 실제 engine release | 기존 ledger 조회·동일 DB 재시작·후속 거래 확정; reverse 방향은 별도 판정 |
| UPD-03 | intent 기록, stop, switch, start, verify 각 전후에서 CLI/호스트 중단 | journal·pointer·서비스·engine 대조 후 안전 상태, 자동 downgrade/재시작 없음 |
| UPD-04 | stop timeout·잔존 process·부분 switch | release 교체/새 start 차단, 마지막 확정 상태와 복구 필요 표시 |
| TOOL-01 | source/Gradle 없는 공식 inspect, Node 동시 접근 | 별도 JVM·JSON/exit code, 기존 DB만 사용, 독점 조건 거부 |
| TOOL-02 | repair 확인 누락·손상·예산 초과·commit후 응답 단절 | 제한된 복구·postCommitAudit, 결과 불명 보존, 자동 재실행/Node start 없음 |

TOOL은 첫 지원 backend인 RocksDB에서 profile별로 인수한다. NIGO-05는 M5/G2의 선행이며 G1-M/L/D의 선행이 아니다. H2 결과는 공급자 evidence 또는 별도 지원 matrix로 기록한다. GC command wrapper는 현재 M5 범위에 포함하지 않는다. 후속 추가 시 command ID·receipt 보존·결과 불명·actor 연결을 별도 인수한다.

## 5. Evidence 형식

`results/<candidate-id>/`에는 `summary.json`, `environment.json`, `cases.json`, 정제한 로그·관측, 수동 재현 기록을 둔다. 개발용 상세 trace는 제한된 위치에 두고 고객·공개 release evidence와 분리한다.

공통 필드: 제품 commit/dirty 여부, archive SHA-256 또는 image digest, engine source/JAR/manifest hash, runtime 공급자·version/hash, Rust/toolchain·Cargo.lock 식별, harness revision, contract revision, profile(G1-M/L/D), OS/kernel/CPU·서비스 adapter, backend/role, config fingerprint, run ID·시작/종료 시각, 각 test의 PASS/FAIL/NOT_CHECKED와 evidence 경로. libc/systemd는 Linux, 로그인 세션/LaunchAgent는 Mac, base image/runtime/volume/network는 Docker evidence에 추가한다.

mock·real engine·target platform 결과를 구분하고 이전 revision 결과를 현재 통과로 복사하지 않는다. evidence 자체에 private key·password·token·전체 환경변수·고객 config를 넣지 않는다. 실패 시에도 같은 sanitizer를 적용한다.

2026-09-16 Go 기반 진단·테스트 결과는 당시 revision/toolchain의 과거 증거로 보존한다. Rust 검증과 Mac 실제 설치·엔진 인수는 새 결과로 기록한다. Go archive 호환 검사는 이전 package 읽기 호환의 증거이며 과거 Go 결과를 Rust 전체 회귀 통과로 승격하지 않는다.

## 6. CI와 릴리스 흐름

1. **PR 검사:** Rust fmt/clippy/tests·Cargo.lock, schema/golden fixture, artifact 부정 사례·기존 Go archive 호환, 문서 링크, secret 패턴을 검사한다.
2. **공급 확보:** 빌드 환경에서 engine/runtime lock의 exact asset을 받고 출처·hash를 검증한다. credential은 환경의 임시 secret이며 package로 복사하지 않는다.
3. **조립:** 임시 경로에서 allowlist payload만 archive에 포함한다. 순서·권한·timestamp 정규화로 동일 입력의 조립 재현성을 점검한다.
4. **후보 확정:** package hash를 기록하고 서명/검증 metadata를 붙인다. 검증할 후보와 발행할 후보를 동일하게 유지한다.
5. **profile 인수:** 먼저 Mac에서 G1-M을 수행한다. 후속 G1-L은 Linux/systemd VM, G1-D는 지정 container runtime에서 별도 수행한다. G2도 동일 profile로 재인수하며 인터넷 차단은 격리된 설치·실행 구간에 적용한다.
6. **RC 보고:** gate 상태, 지원 조합, 제한, 매뉴얼, SBOM/NOTICE와 근거를 묶는다. 필수 실패/미검사는 승격을 차단한다.
7. **발행:** 승인된 게시 단계에서 이미 검사한 bytes를 immutable 제품 버전으로 승격한다. release 게시 때 재빌드하지 않는다.
8. **사후 확인:** 게시 asset을 다시 받아 hash·manifest·출처 증거를 확인한다. 공급 credential이 고객 실행에 필요 없는지 확인한다.

NIGO의 공식 JAR/manifest와 BXDL의 제품 archive는 서로 다른 release다. 고객 패키지는 engine bytes를 그대로 포함한다. 제품 조립 CI가 NIGO를 checkout/Gradle build하는 흐름을 만들지 않는다. GitHub Actions 사용 여부·runner·비용·스케줄은 BXDL 구현 시 정하며 기존 NIGO 주간 workflow를 변경하지 않는다.

서명 도구·key custody·오프라인 trust root 배포 방식은 BX-014에서 확정한다. 최초 개발 fixture는 정식 signed release로 표시하지 않는다. 단순 SHA256SUMS만으로 출처 인증을 주장하지 않는다.

Mac의 .app/.pkg·Developer ID·notarization·Gatekeeper 전달 경험은 실제 배포 방식이 정해질 때 추가 인수한다. 현재 Ed25519 package 서명은 Apple code signing이 아니다. Docker는 image/base digest·전체 공급물을 별도 검증하며 tar archive 서명만으로 충분하다고 표시하지 않는다.

## 7. G3 고객 운영 승인에서 닫을 항목

- 선택 OS/CPU/profile 조합의 engine release gate: native/provider, network fault, 자원 제한·장기 부하 등 공급자 잔여 항목. Linux/systemd·Docker는 Mac 결과로 대체하지 않는다.
- Mac 같은 UID profile의 신뢰 한계와 로그인/sleep 조건. 강한 제어자/engine 격리가 요구되면 별도 설치 profile로 설계·인수한다.
- 정상 재시작과 별도로 crash·전원 손실·durability 결과 불명 대응의 근거와 운영 한계.
- 실제 운영 CA·key 보관·교체·만료 대응, validator 중복 기동 방지 운영 절차.
- 정지된 데이터/WAL/identity의 일관된 백업·격리 복원 drill, backup password 분리와 RPO/RTO 측정.
- 보존 기간·GC 수동 절차·디스크 경보·용량 정책. 자동 GC/전체 disk bound는 엔진 구현·인수 후만 약속.
- 외부 RPC/관리 노출 시 인증·서버측 권한·감사·우회 접근 차단. 운영자 로그인과 원장 서명 권한 분리.
- 실제 하드웨어·workload에서 합의한 운영 기준과 장애 runbook·지원 책임자.

숫자로 된 TPS·SLO·RPO/RTO를 이 계획에서 추정하여 제품 보장으로 고정하지 않는다. 첫 고객 범위에 필요 없는 기능은 미지원으로 명확히 제외하되, 포함한 기능의 필수 안전 조건은 생략하지 않는다.
