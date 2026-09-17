# NIGO 엔진 연동 계약 계획

- 상태: BXDL의 요청·소비 설계 초안. NIGO 측 수락·구현·릴리스를 주장하지 않는다.
- 연결: [구현 계획](./2026-09-16-implementation-plan.md), [인수 계획](./2026-09-16-acceptance-and-release.md).
- 기준: source `a48d1adef4e9c98dbbb675ec3ff73eee25d00dfb`, 읽은 checkout `17b46c151aa2d848b34364ddc43df9c1a202ee41`.
- 2026-09-17 사용자 결정: 소비자 구현은 Rust, 설치·운용 UX와 실제 package 인수는 macOS Apple Silicon을 먼저 대상으로 한다. Linux/systemd는 후속 서버 profile로 유지한다. [최신 결정](./2026-09-17-rust-macos-first.md)이 2026-09-16 Linux 우선 계획의 실행 순서를 갱신한다.
- 최초 제품 인수는 G1-M(macOS 로컬 운영), 이후 G1-L(Linux 서버·multi-host)로 구분한다. Rust CLI 실행·fixture 통과와 기존 NIGO Mac 실행 근거는 공식 공급물·BXDL 설치/엔진 인수를 대신하지 않는다.

## 1. 현재 실제로 존재하는 경계

| 경계 | 현행 기능 | 제품 소비 시 한계 |
| --- | --- | --- |
| canonical bootJar | 검증한 React console 포함 | 공식 공급 manifest·출처·build identity 계약 보강 필요 |
| `GET /monitor/api/console/bootstrap` | instance/chain/genesis/backend·console capability | clientVersion은 고정 문자열. build identity가 아님 |
| `GET /monitor/api/consensus/health` | DB/network와 독립적인 local runtime 관측 | HTTP 200은 조회 성공. 내부 QBFT 타입 노출 존재 |
| `GET /monitor/api/consensus/progress` | pending/vote/finality 근거, IDLE/UNKNOWN 등 | 전역 quorum 증명이 아니며 원자적 ledger snapshot 아님 |
| `POST /rpc` | Native/ETH JSON-RPC | 운영자/API 사용자 인증이 제공된 것은 아님 |
| GC API·job | opt-in 로컬 수동 GC와 제한된 job/command 기록 | 사용자 actor가 로컬 비인증 값으로 고정, 기업 감사 원장 아님 |
| `StorageIntegrityTool` | 별도 JVM, JSON·exit code, 제한된 commitment repair | 문서 진입점은 Gradle. inspect도 DB open/WAL recovery 가능 |

기존 endpoint를 무조건 새 version으로 교체하도록 요청하지 않는다. 먼저 정확한 DTO·field·nullable·시각·오류 의미를 고정한다. 계약 확장이 필요한 항목만 NIGO에 요청하고 BXDL은 adapter에서 소비한다.

## 2. NIGO-01~06 요청안과 우선순위

아래 ID는 기존 NIGO 설계의 범위 식별자를 재사용한다. 새 `REQ-NNNN`을 임의로 예약하거나 기존 요청 상태를 변경하지 않는다.

| ID | 제공 owner / 실제 요청 결과 | BXDL 소비·인수 | 필요 시점 |
| --- | --- | --- | --- |
| NIGO-01 | NIGO: canonical JAR, immutable 식별, engine commit·dirty 여부·JAR hash·console fingerprint·Java/native/platform·계약 fixture revision manifest | BX-011/014: trusted lock에 exact artifact 고정, manifest/실제 bytes 불일치 거부 | 실제 Mac package 및 G1-M 필수; G1-L에서도 재사용·조합 인수 |
| NIGO-02 | NIGO: 공개 chain description과 node-local/secret 분리, canonical 검증, 명시 fresh init/existing restart, missing target의 fresh fallback 거부 | BX-020~022: render·identity 기록. 잘못된 DB/chain/key/partial init 거부 | Mac init/start와 G1-M 필수; OS와 무관한 엔진 안전 계약 |
| NIGO-03 | NIGO: full Spring/DB open/network/signing 없는 cold 검사, JSON·reasonCode·NOT_CHECKED | BX-023: host 검사와 engine 검사 분리, 무변경·미검사 판정 검증 | 실제 preflight 및 G1-M 필수; Mac 로컬 정적 검사만으로 대체 불가 |
| NIGO-04 | NIGO: 외부 DTO·build/capability·role별 startup/readiness·실패 및 종료 계약, secret-free startup report | BX-024/032: 공통 상태 adapter, engine가 못 뜬 경우도 구조화 오류, 결과 불명 보존 | Mac launchd 연결 및 G1-M 필수; Linux systemd는 G1-L |
| NIGO-05 | NIGO: Gradle 없는 공식 offline 도구 실행 artifact/entry point, 독점 접근·JSON·exit code | BX-051/052: inspect/repair wrapper, Node 동시 실행 거부, postCommitAudit 소비 | 후속 유지보수. G1-M/G1-L의 기본 설치·실행 선행 아님 |
| NIGO-06 | NIGO: 위 제공물의 명세·fixture·supplier tests와 exact revision/hash 연결. 엔진의 file H2/RocksDB·QBFT 증거 구분 | BX-040/041: Mac arm64/RocksDB exact package와 로컬 4-validator 회귀를 먼저 인수, Linux multi-host는 별도 후속. H2는 첫 제품 후보 범위 밖 | G1-M에는 01~04 subset; G1-L은 Linux 조합 추가, 유지보수에 05 추가 |

NIGO-01~04/06은 NIGO `requirements/REQ-0002-bxdl-engine-foundation.md`의 OPEN 초안으로 추적한다. NIGO-05는 같은 상위 주제에서 제공·인수 시점을 분리한다. Rust/macOS 우선 변경은 BXDL 사용자 결정이며 실제 NIGO 담당자·일정·계약 수락은 공급자 응답 후 기록한다. NIGO requirements 변경의 공유 commit/PR은 별도 세션에서 진행한다.

## 3. Artifact·config 계약의 최소 내용

정확한 field spelling은 합의 전이다. 아래 의미가 누락되지 않아야 한다.

- Artifact: 엔진 source revision, dirty 여부, asset hash, console identity, Java 조건, native 요구, 지원/실제 검증 matrix, 계약 version, 공급자 evidence.
- Chain: canonical genesis/profile/validator identity와 공개 설정, shared와 node-local 항목 구분. chain ID 단독으로 identity를 만들지 않는다.
- Instance: role, node/validator 공개 identity, backend·경로·peer/TLS/secret 참조. 키 원문을 manifest에 넣지 않는다.
- Init: 로컬 신규 data 초기화만 수행하는 범위, 기존/부분 data 거부, 생성 identity 반환, 중단 후 판정 절차. 기존 네트워크에 가입할 때 새 네트워크 genesis를 임의 생성하지 않는다.
- Restart: 기존 DB/WAL·key·profile을 검증하고 누락·불일치에서 중단. fresh DB fallback 없음.
- Compatibility: source→target engine·DB/profile 조합별 허용/거부와 실제 증거. 미제공이면 upgrade 미지원이며 자동 migration/downgrade 없음.

engine lock과 NIGO manifest는 공급 산출물을 식별한다. BXDL package manifest는 제품 CLI·runtime·설정 schema·engine lock·payload hash를 함께 식별한다. 두 manifest는 소유권이 다르며 제품이 엔진의 검증 이력을 만들어 채우지 않는다.

개발 manifest의 `darwin/arm64`와 기존 `linux/amd64`는 대상 schema다. 동일 JAR를 소비하더라도 CLI·JRE/native·환경별 hash와 실제 실행 인수는 각각 연결한다. 선정 Mac JRE와 launchd를 포함한 G1-M을 먼저 검증하고, 이를 Linux/native·systemd·multi-host G1-L의 PASS로 재사용하지 않는다.

## 4. 초기화·검사·종료의 부수 효과

| 작업 | 허용 효과 | 반드시 보존할 실패 의미 |
| --- | --- | --- |
| cold preflight | 기존 config/key의 읽기·공개 identity 도출, host 정적 관측 | DB 내부 상태는 NOT_CHECKED. DB open·schema 생성·listen·송신·signing·키 생성 없음 |
| explicit init | 확인한 새 instance의 데이터 생성; 해당 profile의 engine 계정으로 공식 도구 실행(Mac 사용자 / 후속 Linux 서비스 UID) | 기존 DB overwrite 금지. partial init은 보존하고 engine 판정 없이는 재실행 금지 |
| existing start | 기존 데이터 검증, 허용된 WAL recovery, 네트워크 시작 | 경로 오타·profile/key 불일치에서 새 chain 생성 금지 |
| graceful stop | timer/network/signing 중지와 storage close | timeout, fail-stop, durability 결과 불명은 정상 종료와 구분 |
| offline inspect | 기존 DB exclusive open, 검사; backend에 따라 물리 복구/쓰기 가능 | 무변경 검사로 표기하지 않음. 대상 없음/lock 충돌/close 오류 거부·보고 |
| limited repair | 엔진이 정의한 commitment 범위 수정 | 이미 commit됐을 가능성과 postCommitAudit 실패를 보존. 자동 재시도 없음 |

일반 Spring 기동은 schema를 기록하고 빈 DB에 genesis를 생성할 수 있다. `auto-start=false`, H2 durability preflight, RocksDB `openExisting`을 cold 검사로 재사용하지 않는다. preflight 통과 뒤에도 engine startup 검증과 독점 접근이 필요하다.

init 성공 후 BXDL manifest 기록 전에 중단되는 경우를 NIGO-02에 포함한다. 데이터 존재만으로 성공이나 실패를 단정하지 않는다. 부수 효과 없는 검증 또는 별도 명시 offline 판정의 허용 범위를 공급자가 정의하고, 그 전에는 `INITIALIZATION_INCOMPLETE`로 start를 막는다.

## 5. 관측·오류 계약

외부 DTO의 nullable, unknown enum, 큰 정수 표현, wall-clock/monotonic 의미, 관측 시각·instance reset 규칙을 fixture로 제공받는다. 엔진 내부 Java class 이름이나 로그 문장 parsing을 장기 계약으로 사용하지 않는다.

BXDL이 필요한 관측은 process alive, local engine initialization, role별 readiness, sync 상태, 관측 가능한 finality 근거다. 이 항목을 단일 healthy boolean으로 압축하지 않는다. validator/observer/manual-start·INSTANT를 구분하고 observer에 engine가 없거나 무거래 IDLE인 상태를 장애로 표시하지 않는다.

제안 startup report의 최소 의미는 instance/build identity, phase, stable reasonCode, retry 가능성/복구 필요, 비밀정보 없는 설명이다. 실제 endpoint/파일/stdio 방식은 NIGO-04 합의 대상이다. report가 없는 현재 build는 로그를 첨부한 `UNCLASSIFIED_STARTUP_FAILURE`로 표시할 수 있으나 안정된 오류 계약 인수 완료는 아니다.

종료 계약은 최소한 process 종료와 storage/safety 결과를 구분해야 한다. 원장 상태를 직접 읽어 BXDL이 안전 종료를 추측하지 않는다. 도구가 crash하거나 JSON을 끝내지 못하면 exit code만으로 transaction 성공/실패를 확정하지 않는다.

launchd job 제거, systemd stop 응답, 컨테이너 종료는 각각의 플랫폼 관측이다. 공통 엔진 DTO에 unit/InvocationID/cgroup을 필수로 요구하지 않는다. Mac의 로그아웃·sleep/wake·종료 timeout과 Linux의 서비스·재부팅 조건은 별도 adapter 인수에 남기며, 엔진 종료 계약은 공통으로 소비한다.

## 6. 계약 fixture·버전 관리

`contracts/nigo/<contract-version>/`에 schema, 정상·실패 fixture, nullable/unknown 사례, source manifest를 저장하는 안이다. 모든 fixture에는 제공 commit과 `PROPOSED`/`SUPPLIER_DELIVERED`/`CONSUMER_VERIFIED` 수준을 기록한다. 실제 임의 고객 응답·secret을 fixture로 commit하지 않는다.

처음에는 고정 revision adapter를 사용한다. 미지원 계약은 명시 거부하며 필수 field 누락을 default 정상값으로 메우지 않는다. schema version 변화 없는 의미 변경도 회귀로 검출하도록 golden fixture와 실제 artifact contract test를 함께 둔다. protocol/DB schema version을 제품 계약 version과 혼동하지 않는다.

Supplier test는 source engine의 계약을 검증하고, consumer test는 제공받은 JAR를 검증한다. NIGO managed E2E의 fresh build를 BXDL package gate에서 실행하지 않는다. 새 엔진 조합을 채택할 때 lock 변경·adapter 영향·실제 인수 결과를 같은 변경에 연결한다.

## 7. 요구 전달 절차와 미합의 항목

1. NIGO `requirements/README.md`와 인덱스의 최신 상태를 확인한다.
2. BXDL 구현 범위·공급물·우선 완료 기준을 적어 요구 초안을 만든다. 기존 REQ-0001은 문서 인계임을 유지한다.
3. 권한 있는 구현 단계에서 NIGO에 문서 PR 또는 합의한 경로로 전달하고 실제 REQ 번호·revision을 기록한다.
4. NIGO owner가 제공한 artifact·fixture·실행 근거를 받은 뒤 BXDL이 지정 조합으로 인수한다.
5. `DELIVERED`와 `VERIFIED`, source regression과 package acceptance를 구분한다.

현재 미합의: init/cold 도구 실제 호출법, manifest·DTO schema, startup/shutdown report, 공식 release 공급 일정, compatibility 범위, Mac 및 후속 Linux JRE 공급자/patch, signed package trust root. REQ-0002는 로컬 OPEN 초안이며 이 문서 갱신은 원격 제출·공급자 수락·공식 플랫폼 지원을 뜻하지 않는다.

원격 운영은 별도 계약이다. 현재 P2P mTLS를 운영자 로그인으로 간주하지 않는다. GC의 고정 local actor를 전달받은 임의 user 문자열로 바꾸는 것만으로 인증을 완성하지 않는다. 인증된 service identity·실제 사용자·engine job/command의 신뢰 가능한 연결과 raw RPC 접근 차단을 함께 설계한다.

## 8. 근거 파일

NIGO 기준 revision의 다음 파일을 참조했다.

- `design/2026-09-16-bxdl-product-distribution-and-engine-contract-design.md`, `requirements/REQ-0001-bxdl-bootstrap-handoff.md`.
- `nigo-java/nigo-node/src/main/java/org/nigo/node/initializer/GenesisBlockInitializer.java`.
- `nigo-java/nigo-node/src/main/java/org/nigo/node/ProtocolApplication.java`.
- `nigo-java/nigo-node/src/main/java/org/nigo/node/consensus/runtime/H2QbftDurabilityBarrier.java`.
- `nigo-java/nigo-node/src/main/java/org/nigo/node/monitor/dto/ConsensusHealthDto.java`.
- `nigo-java/nigo-node/src/main/java/org/nigo/node/monitor/service/MonitorConsoleBootstrapService.java`.
- `nigo-java/nigo-node/NODE_CONFIGURATION.md`, `QBFT_DEVNET_RUNBOOK.md`, `STORAGE_INTEGRITY.md`, `CONSOLE_GC.md`.

운영 문서의 시점 차이는 source와 최신 task evidence로 확인한다. 예를 들어 오래된 RocksDB 운영 문구의 GC 미구현 설명을 현재 양 backend 수동 GC 구현 전체의 부재로 해석하지 않는다. 자동 GC·전체 disk bound는 별도 상태다.
