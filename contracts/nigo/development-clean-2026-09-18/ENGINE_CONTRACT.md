# BXDL 개발 후보 엔진 실행 계약

[Node](README.md) · [요구](../../requirements/REQ-0002-bxdl-engine-foundation.md) · [상태 DTO](ENGINE_RUNTIME_CONTRACT.md)

상태: **PROPOSED / 개발 후보**. NIGO 제공자 구현 명세이며 BXDL 양측 합의·실제 package 인수·정식 release는 별도다.
DB schema/profile이나 합의 wire version을 추가하지 않는다. 기존 인자만 사용하는 Spring 개발 기동은 그대로 둔다.
BXDL은 아래 명시 명령만 사용하며 환경변수·system property·working directory의 Spring 설정을 관리 설정으로 사용하지 않는다.

## 명령

```text
java -jar nigo-node.jar engine-info
java -jar nigo-node.jar preflight --config=/absolute/path/node.json
java -jar nigo-node.jar init --config=/absolute/path/node.json --report=/absolute/path/init.jsonl --attempt-id=init-001
java -jar nigo-node.jar resume-init --config=/absolute/path/node.json --report=/absolute/path/resume.jsonl --attempt-id=resume-001
java -jar nigo-node.jar run --config=/absolute/path/node.json --report=/absolute/path/run.jsonl --attempt-id=run-001
```

`engine-info`는 Spring context 없이 포함된 build identity를 반환한다. 일반 classes/test 실행의 resource 부재는 MISSING이며
정식 식별정보로 사용하지 않는다. `preflight`는 설정·키 자료 읽기만 수행하고 DB open·네트워크·서명·키 생성은 하지 않는다.
성공한 정적 검사에도 DB/WAL·peer/port·native 실행은 NOT_CHECKED이므로 전체 INCOMPLETE/exit 3이다.
그 결과로 실제 기동 검증을 생략하지 않는다. mTLS material 로딩 성공은 handshake·peer 인증·expiry 검증 통과가 아니다.

## 입력

node.json은 다음 네 필드만 허용한다. 경로는 이 파일 기준으로 절대경로 정규화한다. symlink 경로는 거부한다.
아래 INSTANT 예시는 최초 smoke fixture이며 고객 QBFT 구성 기본값이 아니다.

```json
{
  "chainFile": "chain.json",
  "dataDirectory": "data",
  "backend": "rocksdb",
  "node": {
    "server.address": "127.0.0.1",
    "server.port": "18080",
    "nigo.monitor.console.enabled": "true"
  }
}
```

chain.json은 공개 chain properties의 flat JSON이다. literal scalar만 허용하며 중복 JSON key·placeholder·환경 치환은 거부한다.
컬렉션은 Spring indexed property 표기(예: `nigo.protocol.consensus.qbft.validators[0]`)를 사용한다.

```json
{
  "nigo.protocol.chain-id": "11578",
  "nigo.protocol.consensus.protocol": "INSTANT",
  "nigo.protocol.consensus.profile-id": "DEV_INSTANT"
}
```

chain은 chain ID, consensus profile/protocol/parameters/validators, fee revisions, 명시 genesis funding·issuer를 소유한다.
node는 HTTP loopback/port, console 활성화, consensus auto-start, QBFT node/peer/secret 참조·sync 및 ingress permission을 소유한다.
QBFT는 기존 설정 명세의 validator/observer·keystore·password file·mTLS trust/pin을 사용하며 managed 실행에서 plaintext는 허용하지 않는다.
fresh validator와 genesis-only observer도 `sync.enabled=true`로 시작할 수 있다. source에는
`sync.archive.enabled=true`가 필요하며 네트워크 개방 전에 검증된 로컬/genesis archive를 게시한다.
미완료 snapshot 승격 때문에 초기 게시가 불가능하면 검증된 catch-up 뒤 재게시하고, 이 최종 게시 실패는 기동 실패다.
observer/manual follower는 source 단절 시 현재 인증된 다른 source에서 durable checkpoint 다음부터 재개한다.
실행 중 snapshot 덮어쓰기나 validator 재활성화는 하지 않는다. local apply/storage/cleanup 실패는 자동 재시도하지 않는다.
실제 공급자 종료·후속 finality·동일 DB/key 재시작의 검증 범위와 결과는 연결된 작업 기록을 따른다.
인증 전 HTTP는 loopback만 허용한다. 파일 경로는 `${...}` 또는 임의 Spring/JDBC 인자로 우회할 수 없다.
backend는 `rocksdb` 또는 로컬 `h2`이며 `dataDirectory/ledger`를 사용한다. H2 URL은 엔진이 생성하고 원격/INIT 옵션은 입력받지 않는다.
첫 H2 개발 후보는 OS 계정/파일 접근 경계를 사용한 sa/빈 DB password이며 원격 H2 console을 비활성화한다. 암호화·사용자 인증 제공이 아니다.

## 초기화·재시작·중단

- init: 부모 디렉터리가 준비된 신규/빈 dataDirectory만 사용한다. engine lock과 INITIALIZING journal을 먼저 만들고 기존 canonical genesis 초기화를 호출한다.
  HTTP/P2P·sync·consensus 서명을 시작하지 않는다. 저장소 종료까지 확인한 뒤 INITIALIZED와 genesisHash를 기록한다.
- run: INITIALIZED journal, 동일 chain fingerprint/node identity/backend/data 경로와 기존 ledger가 필요하다.
  missing/partial/wrong 경로를 새 DB로 만들지 않는다. genesis identity는 DB owner 생성 뒤 dependent 초기화보다 먼저 확인한다.
- resume-init: INITIALIZING journal과 기존 ledger가 있을 때만 명시 실행한다. 기존 genesis/profile 검증을 수행하고 저장소 종료 후 INITIALIZED를 게시한다.
  genesis가 없는 부분 초기화는 거부하고 보존한다. 깨진 DB를 새로 만들거나 자동 repair하지 않는다.
- dataDirectory의 `engine.lock`과 `engine-instance.json`은 노드 로컬 관리 기록이지 새 ledger schema가 아니다.
  init 이후 파일을 임의 제거·이동·복제하여 instance identity 검증을 우회하지 않는다. 같은 OS 사용자에 대한 보안 격리 수단도 아니다.
- 애플리케이션/DB 파일 변경과 journal 게시 사이 crash는 결과 불명이다. journal의 명시 상태·기존 ledger 검증 없이 자동 재시도하지 않는다.
  일반 process crash·power-cut 내구성 검증이나 임의 기존 개발 DB의 자동 등록/마이그레이션을 제공하지 않는다.

## 결과와 종료

stdout은 JSON command result, JVM 로그는 stderr다. report는 dataDirectory 밖의 **새 파일**이며 기존 파일을 덮어쓰지 않는다.
append-only JSONL 각 행에는 attemptId, command, sequence, pid, observedAt(epoch millis), status, reason, contractStatus, details가 있다.
attemptId는 `[A-Za-z0-9_-]{1,80}`이며 BXDL job ID와 별도로 정확한 실행 시도에 연결한다. 미완성 마지막 행은 확정 결과가 아니다.

| 상태 | 의미 |
| --- | --- |
| CHECKING / STARTING | 정적 확인 / 실제 storage·application 시작 진행 |
| INITIALIZED | init 또는 resume-init의 genesis 검증·storage close·instance journal 게시 완료 |
| RUNNING | 로컬 startup 반환; sync 완료·전역 quorum 보장이 아니므로 역할별 health와 별도 관측 |
| STOPPING | 종료 요청 관측, 종료 완료 아님 |
| STOPPED | Spring 종료 순서에서 consensus stop과 backend owner close가 확인됨 |
| FAILED | 시작·초기화 실패. 결과가 부분 반영됐을 수 있으므로 자동 삭제/재시도 금지 |
| UNKNOWN | 종료·보고 확인 불가. terminal report 부재·timeout·SIGKILL도 UNKNOWN으로 소비 |

exit 0은 one-shot 성공, 3은 cold 검사 INCOMPLETE, 64는 입력/설정 오류, 74는 I/O·precondition·기동 실패다.
init/resume-init은 exit 0·stdout 결과·동일 attempt의 마지막 완전한 report가 모두 일치해야 성공으로 소비한다.
stdout 결과는 자원 종료 뒤 한 번만 출력하며 충돌·보고 유실은 UNKNOWN이다.
run의 OS signal exit code만으로 정상 종료를 판정하지 않는다. 정확한 attempt의 terminal report와 서비스 manager 관측을 함께 확인한다.
startup 보고 이전 오류는 secret-free stdout 오류가 남거나 프로세스 시작 자체가 불가능할 수 있다. 파일 부재를 성공으로 처리하지 않는다.

QBFT follower·sync serving·timer/signer·transport/gossip owner는 실제 thread/executor 종료를 확인한다.
필수 stop/drain과 backend close가 확인되고 report 쓰기가 성공했을 때 `STOPPED / STORAGE_AND_CONSENSUS_CLOSED`다.
timeout·interruption·close 실패는 `UNKNOWN`이며 반복 stop으로 성공을 만들지 않는다.
`details.storageClosed`, `details.consensusStopReturned`는 성공 경로에서 확인한 값을 제공한다.
power-cut 내구성, OS service manager 종료, BXDL 선정 package/JRE 인수는 별도다.
`RUNNING.details.nodeInstanceId`는 bootstrap의 `nodeInstanceId`와 같고 재기동 시 바뀐다.

## 산출물과 검증

`nigo-java/`에서 Java 21과 [고정 Node/npm](MONITOR_CONSOLE_BUILD.md)을 사용한다.

```bash
./gradlew :nigo-node:engineDistribution -PconsoleNode=/absolute/node-distribution/bin/node
./gradlew :nigo-node:testEngineArtifact -PconsoleNode=/absolute/node-distribution/bin/node
./gradlew :nigo-node:testEngineArtifactIdentity
```

`engineDistribution`은 mandatory fresh console gate를 통과한 canonical JAR·`<JAR 이름>.sha256`·`engine-manifest.json`을
`nigo-node/build/distributions/engine-development/`에 만든다. `testEngineArtifact`는 이 산출물을 새로 생성하고
manifest의 파일명·hash·size를 확인한 뒤 **해당 JAR**로 `EngineCommandIntegrationTests`를 실행한다.
일반 `test`의 source classpath 실행과 구분하며, `testEngineArtifactIdentity`는 test-owned ZIP의 누락·불일치 거부만 검증한다.
테스트 task 제공이나 fixture 포함 자체가 실행 통과 근거는 아니며 실제 결과는 아래 작업 기록에 남긴다.

JAR의 `engine-build.json`과 manifest는 source commit/dirty, Java 요구, console fingerprint 및 PROPOSED 계약 identity를 연결한다.
계약 입력 순서는 `ENGINE_CONTRACT.md`, `src/main/resources/engine/contract.json`, `ENGINE_RUNTIME_CONTRACT.md`,
`src/test/resources/engine-contract/health-cases.json`이다. manifest의 `contractInputs`는 각 repository-relative `sourcePath`,
JAR entry와 SHA256을 제공한다. aggregate fingerprint는 이 순서대로 `UTF-8(sourcePath) + 0x00 + 원문 bytes + 0x00`을
연결한 SHA256이다. 정의 JSON은 `BOOT-INF/classes/engine/contract.json`, 문서 2개와 health fixture는
`BOOT-INF/classes/engine/contract-evidence/`에 포함하며 packaged bytes를 검증한다.

출처 metadata는 정상 Gradle dependency graph 사용을 전제한다. compile/resource/JAR task를 `-x` 등으로 건너뛴 산출물을
source 일치 근거로 사용하지 않는다. dirty 여부는 미커밋 변경의 존재만 알리며 전체 source snapshot hash·재현성·서명된 attestation이 아니다.
배포 디렉터리의 여러 파일 게시도 하나의 원자적 동작이 아니므로 중단 뒤 이전/혼합 파일이 남을 수 있다.
소비자는 신뢰한 lock/manifest를 기준으로 실제 JAR hash·size와 내부 build/console/계약 identity를 확인하고 불일치는 실행 전에 거부한다.
같은 미확인 출처의 checksum만으로 공급자 인증을 주장하지 않는다. 이 경로는 **development candidate**이며 공식 release,
검증되지 않은 Java/native/platform 조합이나 BXDL package 지원 완료를 표시하지 않는다.
같은 artifact의 다른 cwd/공백 경로·양 backend init/run/부정 사례를 공급자가 검증하고 BXDL은 자기 exact package/JRE로 별도 인수한다.
계약 정의는 [contract.json](src/main/resources/engine/contract.json), 진행·실제 결과는
[기반 작업](../../tasks/2026-09-17-bxdl-engine-foundation.md)과 [QBFT 복구·종료 보완](../../tasks/2026-09-17-qbft-managed-runtime-recovery.md)을 따른다.
