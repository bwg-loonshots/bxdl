# RocksDB Node 운영

[Node 안내](README.md) · [설정](NODE_CONFIGURATION.md) · [저장 레이아웃](../nigo-storage-rocksdb/STORAGE_LAYOUT.md)

## 선택과 DB 경로

`nigo-java/`에서 프로젝트 wrapper를 실행한다. 아래 경로는 예시이므로 실제 노드 전용 디렉터리로 바꾼다.

```bash
./gradlew :nigo-node:bootRun \
  --args='--server.address=127.0.0.1 --spring.profiles.active=devnet --nigo.protocol.storage.type=rocksdb --nigo.protocol.storage.rocksdb.path=/absolute/path/to/nigo-rocksdb'
```

`nigo.protocol.storage.rocksdb.path`는 필수다. 상대경로는 실행 JVM의 working directory를 기준으로
절대경로 정규화되므로 재시작·서비스 배포에는 절대경로를 사용한다. 파일시스템 root는 허용하지 않는다.
빈 디렉터리는 fresh DB로 열며, 기존 DB는 지원하는 전체 column-family 집합과 일치해야 한다.
Manifest 없는 nonempty 디렉터리나 다른 layout을 자동 초기화하지 않는다. 다른 프로세스가 열고 있는
같은 DB를 중복 writer로 사용하지 않으며, 다른 노드는 별도 디렉터리를 사용한다.

RocksDB를 선택하면 RDB storage bean 및 Spring JDBC/DataSource·transaction-manager·H2 console 자동구성을 제외한다.
`spring.datasource.url`은 RocksDB 경로를 지정하는 설정이 아니다. 필수 native store를 Memory/RDB로
조용히 대체하지 않는다. 기본값 `rdb` 및 명시적인 `memory` 구성은 계속 별도로 유지한다.

## Genesis·블록·정상 재시작

Node가 생성·검증한 genesis schema, StateCell, commitment nodes와 block 0은 `RocksDbGenesisWriter`의
한 synchronous WAL WriteBatch로 게시한다. 기존 DB 재시작은 저장된 genesis profile을 configured profile과
비교하며, DB를 새 profile/schema로 자동 migration하거나 재초기화하지 않는다. 현행 `OSAKA_V1`/schema `1`을 유지한다.

실행 블록은 `RocksDbNodeBlockCommitter`가 저장 모듈의 coordinator를 호출한다. State·transaction/receipt·
ETH mapping·code·commitment·block/location과 선택적인 finality certificate/checkpoint가 한 native batch에 속한다.
JDBC `@Transactional`로 여러 native 쓰기를 묶는 구조가 아니다. 최근 finalized cache와 Node event publication은
native commit 이후의 별도 후처리다. QBFT validator-local safety는 여전히 서명 메시지 전송 전에 별도로 영속화한다.

정상 종료는 Ctrl-C 또는 서비스의 graceful stop을 사용하고 같은 DB 경로·chain/genesis 설정으로 재기동한다.
Spring이 소유하는 단일 `RocksDbDatabase`가 native handle을 닫는다. 실행 중인 DB 파일을 임의 복사한 결과를
일관된 backup으로 간주하지 않는다. Live backup, process-crash·전원 차단 복구 runbook은 이 구현의 보장 범위가 아니다.

QBFT는 기존 [Runbook](QBFT_DEVNET_RUNBOOK.md)의 identity, trust anchor, peer 및 sync/archive 설정을 함께 사용한다.
`storage.type` 변경만으로 INSTANT 원장을 QBFT 원장으로 변환하지 않는다. Historical sync의 body가 없는 블록은
명시적인 header-only marker로 저장한다. 원래 header/hash/root를 보존하지만 거래 body를 저장했다고 주장하지 않으며,
일반 full-block 저장의 transaction/receipt root 검증을 완화하지 않는다.

## 관측과 비용

Native Asset activity는 receipt input/output·supply와 program scope에서 유도한 membership을 거래와 함께 저장하고,
finalized location 게시 시 asset ID + 역순 block/index key를 같은 batch에 만든다. 조회는 해당 asset prefix에서
`limit + 1`건만 seek한다. 전체 ledger 스캔이나 signed timestamp 정렬을 사용하지 않는다. ZERO 수수료 program의
부수적인 NIGO funding은 제외하되, 직접 NIGO scope 및 실제 NIGO fee는 기존 조회 계약대로 포함한다.

Typed storage metrics는 실제 persisted record count를 최대 1,000개 key씩 읽어 집계한다. Memory는 페이지 단위지만
비용은 전체 key 수에 비례하고 DB owner의 read lock을 잡으므로 고빈도 polling·저장 용량 상한 지표로 사용하지 않는다.
Block count는 historical header-only를 포함한 알려진 header 수다. RDB 전용 normalized row count는 `null`이며,
`integrityChecked=false`/`integrityViolationCount=null`은 전수 integrity audit가 미지원임을 뜻한다.
기존 공통 DTO의 `integrityOk=true`만으로 감사 통과를 판단하지 않는다. 미검사 상태에서도 해당 값은 true다.
논리 record 수와 archive canonical payload bytes는 SST/WAL disk allocation·압축률·filesystem 점유량과 다르다.

## 지원과 검증 경계

- JNI는 `org.rocksdb:rocksdbjni:10.10.1`로 고정한다. 실제 로컬 검증 환경은 macOS/Darwin arm64와 Java 21이다.
- 해당 배포 artifact의 POM에는 Apache License 2.0 및 GNU GPL version 2가 선언되어 있다.
  릴리스 시 선택한 배포 조건·transitive native 구성의 LICENSE/NOTICE와 SBOM을 별도로 확인한다.
- Linux/다른 CPU의 JNI 로딩·release packaging 검증은 환경 대기 항목이다. 의존 artifact 포함을 모든 플랫폼 실행 근거로 보지 않는다.
- Node 기동·정상 종료 후 같은 디렉터리 재시작과 native `beforeWrite` 준비 실패는 일반 process-crash·전원 차단·실제 native I/O fault 검증과 다르다.
- Snapshot promotion은 startup-exclusive bulk WriteBatch이며 총 memory bounded generation switch가 아니다.
- Archive count/byte retention·단일 serving adapter lease는 범용 state root retention, pin/lease, GC/pruning 또는 historical Cell/proof 서비스가 아니다.
  별도의 [root registry·archive pin·reader lease](./STORAGE_LIFECYCLE.md)와 [수동 commitment GC](./CONSOLE_GC.md),
  [기본 OFF 자동 GC 정책](./STORAGE_AUTO_GC.md)이 같은 보존·접근 안전 경계를 사용한다. Cell/원장 삭제나 compaction은 포함하지 않는다.
- RDB→RocksDB 데이터 이동 및 이전 개발 binary의 layout migration은 제공하지 않는다. 출시 전 단일 schema/profile을 파편화하지 않는다.

실행 명령·테스트 및 process evidence의 확정 결과는 [작업 기록](../../tasks/bak/2026-09-07-rocksdb-storage-implementation.md)이 소유한다.
