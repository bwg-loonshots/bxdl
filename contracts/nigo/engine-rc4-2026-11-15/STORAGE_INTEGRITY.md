# 오프라인 Storage 무결성 검사·복구

`StorageIntegrityTool`은 Node와 별도 JVM에서 실행하는 운영 도구다. 같은 canonical Node JAR의
`storage-integrity` 명령으로 호출하며 Java 21 외에 source checkout이나 Gradle이 필요하지 않다.
Spring Boot, genesis, consensus, HTTP/P2P/RPC, signer, 키 생성, 자동 GC와 schema initializer를 시작하지 않는다.
기존 로컬 file H2와 RocksDB만 지원하며 Memory·TCP H2·외부 RDB 서버는 지원하지 않는다.

이 명령은 managed `init`/`run`의 하위 mode가 아니다. node.json·키/PKI·`engine-instance.json`·
managed `--report`/`--attempt-id`를 입력받지 않으며 기존 DB의 명시 경로와 검사 옵션만 사용한다.
관리형 실행의 설정·identity 계약은 [Engine 계약](./ENGINE_CONTRACT.md)과 별도다.

## 실행 순서

1. 해당 DB를 사용하는 Node와 다른 도구를 모두 종료한다. 파일 잠금은 별도 process의 동시 open을 거부한다.
   같은 JVM에 내장하거나 실행 중인 Node의 bean으로 사용하지 않는다.
2. 복구 전에 필요하면 **종료된 DB의 전체 파일을 별도 보관**한다. 이 도구는 백업이나 undo 파일을 만들지 않는다.
3. 정확한 기존 경로로 `inspect`를 실행하고 결과를 확인한다.
4. `repair`는 변경 의도를 확인한 뒤에만 실행한다. 복구 요청도 처음부터 다시 검사하며 이전 report를 신뢰하지 않는다.
5. exit `0`과 온전한 JSON이 일치하는지 확인한다. `audit.status=REPAIRED`이면 별도
   `postCommitAudit.status=HEALTHY`가 필요하다. 이미 정상인 대상은 `HEALTHY` no-op일 수 있다.
   이 확인은 선택 target의 검사 완료이며 다른 기동 조건을 대신하지 않는다. 실패하거나 판단이 불명확하면 Node를 내린 상태로 유지한다.

### 배포 JAR에서 실행

아래 경로는 예시이므로 실제 기존 DB의 **절대·정규화된 경로**로 바꾼다. 상대경로, `.`/`..` 성분과
경로 중 어느 위치에든 symlink가 포함된 경로는 거부한다. H2는 `.mv.db`를 뺀 basename,
RocksDB는 `CURRENT`가 있는 디렉터리를 지정한다. `--backend=rdb`는 도구의 H2 표기이며
managed node.json의 `h2`와 구분한다. 존재하지 않는 DB/CF/schema를 새로 만들지 않는다.

```bash
java -jar nigo-node.jar storage-integrity --backend=rdb --path=/absolute/path/data/ledger --mode=inspect
java -jar nigo-node.jar storage-integrity --backend=rocksdb --path=/absolute/path/data/ledger --mode=inspect

# 대상과 보고서를 확인한 후 별도로 실행
java -jar nigo-node.jar storage-integrity --backend=rocksdb --path=/absolute/path/data/ledger --mode=repair --confirm-repair=true

# 파생 잔액 인덱스만 검사·복구 (RDB에도 동일한 target 사용)
java -jar nigo-node.jar storage-integrity --backend=rocksdb --path=/absolute/path/data/ledger --target=balances
java -jar nigo-node.jar storage-integrity --backend=rocksdb --path=/absolute/path/data/ledger --target=balances --mode=repair --confirm-repair=true
```

`--mode` 생략 시 `inspect`, `--target` 생략 시 기존 `commitment` 검사다. `balances`는 별도로 명시해야 하며,
두 대상을 동시에 복구하지 않는다. `--confirm-repair=true`는 repair에만 허용한다. H2 기본 사용자는 `sa`이고
`--username=<user>`로 변경한다. 암호가 있으면 `NIGO_STORAGE_PASSWORD` 환경변수로 전달한다. 암호를 command line에
넣는 옵션은 없다. 도구는 `application.yml`을 읽거나 DB/CF/schema를 새로 만들지 않으며 임의 JDBC URL/연결 옵션을 받지 않는다.
`NIGO_STORAGE_PASSWORD`는 이 도구의 H2 자격 증명 입력이며 managed node 설정의 환경변수 치환을 허용하는 뜻이 아니다.

### 소스 개발 환경의 대체 진입점

기존 wrapper task도 같은 도구를 실행한다. `nigo-java/`에서 실행하며 동일하게 절대·정규화된 기존 DB 경로를 지정한다.
Gradle은 BXDL 배포 package의 운영 전제 조건이 아니다. Gradle 자체 task 로그·종료와 도구 JSON을 혼동하지 않는다.

```bash
./gradlew :nigo-node:storageIntegrity --args='--backend=rocksdb --path=/absolute/path/data/ledger --mode=inspect'
```

## 결과와 한도

도구는 backend close 후 stdout에 한 개의 JSON 결과를 출력하며 JVM 로그는 stderr다. 정상 검사 문서의 `target`으로
검사 대상을 구분하고 `audit.status`로 상태를 읽는다. 입력/실행 실패 문서는 최상위 `status`를 사용한다.
`audit`에는 상태, materialized identity,
generation, Cell/node 수, 누락/손상 수, 제한된 findings가 포함된다. Cell 원문과 backend 예외·암호는 출력하지 않는다.

| 상태 | 의미 | CLI exit code |
| --- | --- | ---: |
| `HEALTHY` | 한도 내 검사 대상에서 오류를 찾지 못함. 미참조 node 존재와 양립 가능 | 0 |
| `REPAIRED` | 원자적 복구와 transaction 내부 사후 검사 완료. CLI는 commit 후 독립 검사도 실행 | 0, 독립 검사 실패 시 해당 실패 code |
| `CORRUPT` | 누락·손상 또는 state 불일치 발견 | 2 |
| `INCOMPLETE` | 검사 예산 초과 등으로 전체 판정 불가 | 3 |
| `BLOCKED` | identity/maintenance/import 등 안전 조건 불충족 | 4 |
| `INVALID_ARGUMENTS` | 잘못된 옵션·경로 형식 또는 복구 확인 누락 | 64 |
| `UNAVAILABLE` | 기존 DB·symlink 거부, DB open/권한/잠금/I/O/close 또는 결과 출력 완료 불가 | 74 |

성공은 exit `0`과 온전한 JSON의 `audit.status=HEALTHY`, 또는 `audit.status=REPAIRED` 및
`postCommitAudit.status=HEALTHY`가 함께 확인된 경우뿐이다. 정상 target의 repair는 `HEALTHY` no-op이고
`postCommitAudit`가 없어도 된다. 0이 아닌 CLI 결과는 Gradle에서도 실패로 표시된다.
오류가 나도 schema 초기화나 자동 복구를 재시도하지 않는다.
Commit 이후 별도 검사 실패는 이미 수행한 복구를 rollback했다는 뜻이 아니다. JSON의 repair 결과와 `postCommitAudit`를 함께 확인한다.
Open/I/O/commit/close 실패로 완료 여부를 확정하지 못한 repair 요청에는 `repairMayHaveCommitted=true`를 표시한다.
이것은 commit 성공 증거가 아니라 결과 불명확 표시다. Node를 내린 상태에서 다시 `inspect`하여 확인한다.

JSON 출력은 flush와 출력 오류를 확인하며 실패 시 exit `74`로 끝난다. 실패 문서를 다시 출력하지 않으므로
온전한 JSON을 받지 못할 수 있다. DB commit·close 뒤 `REPAIRED`/`postCommitAudit=HEALTHY`가 일부 또는 전부
수신됐어도 exit `74`이면 완료로 확정하지 않는다. timeout·SIGKILL·빈/잘린 JSON·종료 유실·exit/JSON 불일치는
소비자가 `UNKNOWN`으로 보존한다. 실패 결과는 실패대로 보존하고, 손상 판정 exit `2`나 안전 조건 차단 exit `4`를
성공으로 바꾸지 않는다. 자동 repair 재시도·fresh DB 생성·WAL 삭제·자동 downgrade 없이 Node를 멈춘 채 확인한다.

| 옵션 | 기본값 | 허용 최대 |
| --- | ---: | ---: |
| `--max-cells` | 10,000 | 100,000 |
| `--max-nodes` | 100,000 | 1,000,000 |
| `--max-cell-bytes` | 67,108,864 (64 MiB) | 536,870,912 (512 MiB) |
| `--max-findings` | 100 | 1,000 |

모든 한도는 양수다. 한도를 넘으면 repair하지 않는다. `orphanNodes=null`은 미참조 여부를 확정하지 못했다는 뜻이며 0과 다르다.
불완전한 검사에서 count는 확인한 범위의 수다. `findingsTruncated=true`이면 표시되지 않은 추가 finding이 있다.
`reachableNodes`는 보존 graph에서 발견한 hash 수이며 누락된 node도 포함한다. `missingNodes`/`corruptNodes`는
현재 state에서 예상한 node·보존 graph·inventory를 합쳐 분류한 수이므로 `reachableNodes`보다 클 수 있다.
`max-nodes`는 generated map·방문 집합·관측 집합·inventory 각각의 한도이지 전체 I/O 횟수의 상한이 아니다.
입력 byte/node 수 한도는 JVM 전체 heap quota나 wall-clock deadline이 아니다. 재계산·보존 graph·inventory의 객체 메모리와
DB 엔진 메모리는 별도이며, 한도를 크게 올리기 전에 사용 가능한 메모리를 확인한다. 영속 job/중단 재개 기능은 없다.
Cell은 한 개씩 읽고 accepted Cell의 canonical byte 합계를 제한한다. 한도 초과 확인을 위해 protocol 최대 크기의 Cell을
하나 더 읽을 수 있다. Repair는 독점 callback 안에서 최대 3회 audit하고, 성공한 CLI는 commit 후 한 번 더 검사하므로
단일 inspect보다 오래 걸릴 수 있다.

## 무엇을 검사하고 복구하는가

### Commitment (`--target=commitment`, 기본값)

- Active Cell을 순서대로 읽어 현재 commitment 저장소를 사용하지 않고 root를 재계산한다.
- Ledger header·retention·active snapshot·history coverage의 **DB 내부 identity 일치**를 확인한다.
- Latest/recent 및 활성 pin/lease root의 commitment graph에서 content hash와 트리 위치·구조를 검사한다.
- 전체 graph와 node inventory를 확인한 경우에만 미참조 node 수를 보고한다. 삭제하지 않으며 물리 회수 byte로 환산하지 않는다.
- 재계산 root가 기준 root와 같고 보존 트리 전체를 검증할 수 있을 때만, 현재 Cell로 생성된 missing/decode/hash-corrupt node를
  교체한다. 같은 transaction/WriteBatch에서 기존 node generation도 올려 오래된 GC 진단을 무효화한다.

Cell·block/receipt·retention·snapshot manifest·history body/coverage는 수정하지 않는다. 현재 Cell에 근거가 없는 과거 node,
source/root 불일치와 import 잔여는 추측 복구하지 않는다. Snapshot promotion 뒤 ledger 설치가 아직 진행되지 않은 정상
materialized state는 기존 local metadata 규칙으로 구분한다.
Malformed active Cell은 `CORRUPT`/`ACTIVE_SOURCE_INVALID`로 중단한다. Hash가 유효한 연속 shortcut 표현은 허용하고
재작성하지 않으므로, 모든 저장 node가 최소 크기의 canonical representation이라는 보장은 하지 않는다.

### 파생 잔액 인덱스 (`--target=balances`)

잔액 인덱스는 `(asset, owner)`별 fungible active Cell의 합계를 빠르게 읽기 위한 파생 데이터다. 이 검사에서는
저장된 합계를 신뢰하지 않고 원본 active Cell로 다시 합산한다. 정상 거래의 조회·증분 갱신 방식은 바꾸지 않으며,
시작 시 자동 검사나 거래마다 전체 스캔을 추가하지 않는다. 순차·병렬 실행 엔진과도 별도의 오프라인 경로다.

1. 먼저 같은 독점 callback에서 위 commitment/identity 검사가 `HEALTHY`인지 확인한다. 원본 root 불일치,
   손상된 commitment, import 잔여, 예산 초과는 잔액 복구를 차단한다. Commitment 손상은 해당 target으로 먼저
   진단해야 하며, 잔액 복구가 원본 Cell이나 트리를 추측해서 고치지 않는다.
2. 원본 Cell을 재합산하고 전체 저장 인덱스와 대조한다. 누락된 키, 합계 불일치, 잘못된 key/value encoding,
   원본에 없는 비정상 항목을 구분한다. 잘못된 fungible 원본은 일부만 합산하지 않고 복구를 차단한다.
3. 손상이 발견되고 명시 repair가 요청되면 source·identity·inventory를 다시 확인하고, 전체 파생 인덱스를 한
   transaction/WriteBatch로 교체한다. 교체 후 검증 실패는 rollback하며, CLI는 commit 뒤에도 별도 검사한다.

원본 active Cell이 없어도 정상 증분 갱신이 남긴 **합계 0·invalid count 0인 행은 정상**이다. 반대로 금액이 0인
active output이 존재하면 그 키의 인덱스 행이 반드시 필요하다. 정상 index의 repair 요청은 no-op이고, 실제 손상으로
전체 재구축할 때에는 원본에 없는 정상 zero 행도 함께 정리될 수 있다. 새 자산 권한이나 거래 규칙을 추가하지 않는다.

이 target의 `audit`에는 다음 수치가 포함된다.

| 필드 | 의미 |
| --- | --- |
| `commitmentStatus` | 선행 commitment 검사 결과. 수행 전 차단된 경우 null |
| `expectedEntries` / `storedEntries` | 원본 합계 키 수 / 확인한 물리 인덱스 행 수 |
| `missingEntries` | 원본에 있어야 하지만 저장 인덱스에 없는 키 수 |
| `mismatchedEntries` | 원본과 다른 합계 또는 invalid count를 가진 키 수 |
| `corruptEntries` | 잘못된 key/value encoding 등으로 정상 해석할 수 없는 항목 수 |
| `orphanEntries` | 원본에 없는 비정상 항목 수. 전체 대조 불가능하면 null |
| `rebuiltEntries` | 성공한 재구축에서 기록한 합계 키 수. 원본이 비어 있으면 0도 가능 |

불완전한 검사에서는 수치를 전체 수로 해석하지 않는다. 같은 오류가 여러 검사에 영향을 줄 수 있으므로 항목별 수를
더해 물리 행 수를 추정하지 않는다. Finding에는 owner·금액 원문 대신 key fingerprint를 쓰며, 이 hash는 익명화나
비밀정보 보호를 보장하지 않는다. `--max-cells`는 원본 Cell 수뿐 아니라 expected/inventory 항목 수에도 각각 적용한다.
전체 inventory가 한도 안에 들어오는지 확인하지 못하면, 원본이 비어 있어도 전체 삭제·교체하지 않는다.

복구는 Cell·block/header·receipt·root·commitment node와 generation·history를 변경하지 않는다. schema/CF/profile
버전을 추가하지 않으며, commitment target의 generation 증가와 구분된다. DB 독점 시간, CPU·메모리·I/O는 검사·복구
실행 중에 발생한다. 운영 크기의 중단 시간을 보장하는 도구는 아니므로 데이터 크기와 한도를 확인하고 정비 시간에 실행한다.

### 판정의 범위

`HEALTHY`는 선택하지 않은 다른 보조 인덱스나 모든 block/transaction/receipt/history body의 전수 검사,
외부 기대 chain/genesis·peer/finality·node.json chain fingerprint·engine instance journal 인증, 백업의 유효성,
physical disk 무결성·용량 한도나 장애 없는 미래 운영을 뜻하지 않는다. DB 엔진은 검사 open 시에도 파일 lock/로그/WAL recovery를
수행할 수 있다. Inspect는 application row/CF를 변경하지 않는다는 의미이지 물리 파일의 완전 무쓰기가 아니다.

도구는 DB 내부 ledger/root/retention/snapshot/history identity를 검사한다. `engine.lock`을 획득하거나
`engine-instance.json`의 managed identity를 인증하지 않으며, `HEALTHY`로 managed `run`의 A2 identity 검사를
생략하거나 부분 init을 완료 처리할 수 없다. 물리 backend open의 독점은 검사·복구·독립 사후 검사·close까지 유지하지만
서비스 manager의 자동 재시작을 제어하지 않는다.

BXDL은 대상 DB 경로 선택과 관리형 `dataDirectory/ledger` mapping, 모든 DB 사용 서비스의 중지·재시작 제어,
실행 의도 확인, JAR/JRE 고정, process 종료·stdout JSON의 일치 검증 및 운영 작업 기록을 소유한다.
NIGO 도구는 기존 bounded 검사·복구 결과를 제공하며 새 업무 규칙, DB migration이나 관리형 소유권 전환을 추가하지 않는다.

공통 알고리즘은 [Lifecycle 무결성 계약](../nigo-storage-lifecycle/INTEGRITY_RECOVERY.md), 저장 원자성은
[RDB](../nigo-storage-rdb/INTEGRITY_RECOVERY.md)·[RocksDB](../nigo-storage-rocksdb/INTEGRITY_RECOVERY.md)가 소유한다.
Monitor UX·수동 GC·자동 GC와는 독립적인 오프라인 경로다.

## 공급 및 검증 범위

동일 JAR 명령 추가는 기존 RC.2 JAR·tag의 bytes를 변경하지 않는다. 이 진입점을 포함한 새 JAR의 build identity와
공급 근거로 지원 여부를 확인한다. 기존 source classpath 도구 회귀나 관리형 CLI packaged 회귀만으로 새 진입점의
실제 JAR 실행을 입증하지 않는다. 양 backend·양 target의 source-free 호출, 실제 잠금 충돌·repair·독립 사후 검사와
부정 사례, 이후 source fixture Node 재개방·proof 검증은 [활성 REL-07 작업](../../tasks/active/release-validation.md#rel-07--공식-offline-도구운영-연동-공급)의
실제 수행 근거로 구분한다. 근거 생성 전에는 packaged 동작을 미검증으로 둔다.
fixture 재개방은 BXDL의 동일 JAR 관리형 `run`·서비스 manager 재시작 인수가 아니다.
NIGO 공급자 검증이 통과해도 BXDL 선정 package/JRE와 운영 wrapper의 A7 소비 인수 완료를 대신하지 않는다.

개발자는 `nigo-java/`에서 다음 별도8-case gate로 fresh canonical JAR의 두 backend·target을 검증한다.
Node/npm 고정·Console gate를 포함하며 기존 외부 RC의9-case gate를 대체하거나 변경하지 않는다.

```bash
./gradlew :nigo-node:testEngineStorageArtifact -PconsoleNode=/absolute/node-distribution/bin/node
```
