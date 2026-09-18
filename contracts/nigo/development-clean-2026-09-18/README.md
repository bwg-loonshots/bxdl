# NIGO clean 개발 후보 수신 — 2026-09-18

BXDL 구현 세션이 NIGO의 clean 개발 후보를 읽고 무결성을 확인한 기록이다. 계약 원문 4개, 원본 manifest와 제공자가 정제한 evidence JSON 3개를 byte 단위로 보존한다. 원문 내부의 상대 경로는 NIGO 저장소 또는 공급 bundle 기준이며 수정하지 않았다. 이 README만 BXDL이 작성했다. [기존 dirty 후보](../development-2026-09-17/README.md)의 원문·증거는 보존한다.

계약은 `PROPOSED`, 채널은 `development`, `officialRelease=false`다. 이번 결과는 **지정 clean 후보의 정적 수신·무결성 확인**이다. 양측 계약의 최종 합의, BXDL 선정 JRE/package·launchd·G1-M 인수 또는 정식 공급 승인으로 해석하지 않는다. 이 디렉터리의 존재만으로 제품 코드가 임의 JAR/manifest를 승인하지 않으며, 검토해 선택한 외부 lock과 실제 bytes를 대조해야 한다.

## 수신 identity와 위치

| 항목 | 확인한 값 |
| --- | --- |
| 수신 owner / 날짜 | BXDL 구현 세션 / 2026-09-18 |
| 읽은 NIGO 문서 checkout | `49d1cefcfb200f8eb04b6f9961c889da21f81018` — PR #119 병합 |
| 인계 문서 | `requirements/REQ-0002-clean-candidate-handoff.md` |
| JAR source / 계약 revision | `303e163a9b3f293fa39e42d02b8daa1843973c14` — PR #118 병합 기반 |
| JAR source dirty | `false` |
| JAR 파일명 / 크기 | `nigo-node-0.0.1-SNAPSHOT.jar` / 159,988,739 bytes |
| JAR SHA-256 | `dd5a366ee990d58ff4fa7da89812f66ec9025443229027bffe1b648cab6d6455` |
| Archive 파일명 / 크기 | `nigo-engine-303e163a-dd5a366e.tar.gz` / 153,788,572 bytes |
| Archive SHA-256 | `eef353dd139869489469e9f1eb527719aa8b5b1df147a46d0c4226eb49441707` |
| 계약 fingerprint | `671dc4052b750b13255804e8aa208f37044b7a0fc2800013ae139884edb3ca13` |
| Console fingerprint | `aa813cd89e0455d3e049075d31fa74bf86e94a3e3d5b8fb6c4190a52e393f8d1` |
| Java 요구 | major 21; BXDL의 정확한 공급자·patch·runtime inventory는 별도 선정 |

원본은 같은 호스트의 NIGO 저장소 기준 다음 위치에 있다. `build/` 밖에 보존된 공급본을 읽었으며, NIGO source를 다시 빌드하지 않았다.

```text
nigo-protocol/artifacts/engine-candidates/
  303e163a9b3f293fa39e42d02b8daa1843973c14/
    nigo-engine-303e163a-dd5a366e.tar.gz
    dd5a366ee990d58ff4fa7da89812f66ec9025443229027bffe1b648cab6d6455/
```

문서 게시 HEAD와 JAR source commit은 다르다. clean은 공급자가 빌드 전후 확인한 해당 source 상태이며, 재현 가능한 byte 동일 build나 서명된 공급망 attestation을 뜻하지 않는다. 로컬 파일 접근을 확인했으며 GitHub Release·원격 다운로드 공급을 주장하지 않는다. JAR, archive, JRE, DB/WAL, 키·비밀번호·PKI, 전체 설정·미정제 로그는 BXDL 저장소에 복사하지 않았다.

## 보존한 원문과 hash

앞의 계약 4개 순서는 원본 manifest의 `contractInputs` 순서다. 파일을 정렬·재직렬화하지 않았다.

| 스냅샷 | 공급 bundle 내 원래 경로 | SHA-256 |
| --- | --- | --- |
| [ENGINE_CONTRACT.md](./ENGINE_CONTRACT.md) | `nigo-java/nigo-node/ENGINE_CONTRACT.md` | `73785847368f341dcced20906ae92eb5f01c4fc493faa5768589786d293fe503` |
| [contract.json](./contract.json) | `nigo-java/nigo-node/src/main/resources/engine/contract.json` | `bffe91e25a3c58db7626fadbb8333b5216ab293b7760dee033f2fdf974841687` |
| [ENGINE_RUNTIME_CONTRACT.md](./ENGINE_RUNTIME_CONTRACT.md) | `nigo-java/nigo-node/ENGINE_RUNTIME_CONTRACT.md` | `ec52f844e9291f6a3cd17a4d93e58a71c2acd651e1706c9c7f9b53b6842cf43e` |
| [health-cases.json](./health-cases.json) | `nigo-java/nigo-node/src/test/resources/engine-contract/health-cases.json` | `c64d4003e8b7dba63845f17dfe1386546988640e1af61fba478206c83402c01e` |
| [engine-manifest.json](./engine-manifest.json) | `engine-manifest.json` | `5630db5b9359d488b23c9d88f04cc94d718b83dbda964593d0d1f57da4dbd0ae` |
| [supplier-evidence.json](./evidence/supplier-evidence.json) | `evidence/supplier-evidence.json` | `46cc18630525fed375e621820a0ed79f6ad9244652d87d6f79e438cde035b0ff` |
| [test-summary.json](./evidence/test-summary.json) | `evidence/test-summary.json` | `f42405f33833b5ebbc430c10ae4cdb3edc39837ad4c5d02fee932eba07a94f4c` |
| [engine-info.json](./evidence/engine-info.json) | `evidence/engine-info.json` | `1406f21ec8b9bbb85ec140fa452e6aa9c1d2a80e639114a5ab52fa1aa36f62bb` |

## BXDL이 이번 수신에서 확인한 것

- 인계 문서의 archive/JAR SHA-256·크기가 실제 bytes와 일치한다.
- Archive를 추출하지 않고 읽어 16개 일반 파일의 bytes가 보존된 공급 디렉터리와 일치함을 확인했다. `SHA256SUMS`의 15개 항목은 자기 자신을 제외한 전체 파일 inventory와 일치하며, 각 hash를 확인했다.
- JAR의 build·console·계약 identity entry가 각각 하나이며 manifest의 대응 값과 일치한다.
- 계약 4개는 공급 bundle, JAR 내 entry, source revision `303e163a…`, 읽은 checkout의 동일 경로와 byte 단위로 일치한다.
- Aggregate는 `contractInputs` 순서대로 `UTF-8(sourcePath) + 0x00 + 원문 bytes + 0x00`을 연결한 SHA-256이며 manifest fingerprint와 일치한다.
- 제공자가 보존한 `evidence/engine-info.json`의 `AVAILABLE` 및 identity는 내장 build metadata와 일치한다. 이는 저장된 제공자 출력의 정적 대조이며 BXDL이 Java를 실행한 결과가 아니다.

이 수신 기록 작업에서 Java/노드·DB·네트워크를 실행하거나 NIGO를 수정하지 않았다. 후속 BXDL 실제 호출 결과는 [구현 상태](../../../docs/implementation-status.md)와 연결된 별도 evidence에 기록하며, 원본 manifest/evidence를 사후 수정하지 않는다.

## 공급자 검증과 소비자 경계

공급자 evidence는 이 clean JAR hash에 연결된다. macOS 26.6.2 arm64 / Oracle Java 21.0.7+8-LTS-245에서 source 898건(Node 728, block-sync 136, gossip 34), exact JAR 9건, console 671건 및 i18n 5건, ZIP identity 16건·아키텍처 검증이 통과했다고 기록한다. `test-summary.json`은 counter·testcase 이름/class/time만 포함하고 원문 process report·finality 값·JVM 로그를 내보낸 자료가 아니다.

Exact-JAR 범위에는 file H2/RocksDB 각각 명시 init/run/stop/restart·중단 초기화 재개, fresh 4-validator+genesis-only observer, 실제 source 종료·인증된 다른 source로 전환·동일 finalized 높이 hash/root, 동일 DB/key validator 및 manual follower 재시작과 소유 process의 `STOPPED` 확인이 포함된다. 공급자 시험을 BXDL 선정 runtime/package·launchd·오프라인 G1-M 결과로 승격하지 않는다.

원본 manifest의 `supplierTests=NOT_RECORDED`, `consumerAcceptance=NOT_RUN`, `testedPlatforms=[]`, native `NOT_CHECKED`를 보존했다. 별도 supplier evidence의 `consumerReceipt=NOT_ACKNOWLEDGED`도 공급자가 묶음을 만든 당시 값이며 이 수신 README와 구분한다. 원본 값을 고쳐 양측 합의나 제품 통과로 표시하지 않는다.

## 기존 후보와의 차이 및 다음 소비

기존 BXDL 후보 `37070f…`의 source `aee1cbe5…/dirty=true`에서 이번 `dd5a366e…`의 `303e163a…/dirty=false`로 바뀐다. source/계약 revision, JAR hash·size와 계약 4개 hash/fingerprint를 모두 새 lock에 연결해야 한다. Java major·console identity·PROPOSED/development 경계는 같다. 중간 dirty 후보 `d56c89…`는 같은 새 계약 fingerprint를 사용하지만 이번 clean JAR과 bytes·source identity가 다르므로 시험 근거를 혼용하지 않는다.

새 계약은 follower의 `NEGOTIATING/DOWNLOADING/RETRYING/FOLLOWING/FAILED` 관측과 실제 worker drain에 근거한 `STOPPED`를 제공한다. `READY/FOLLOWING`이 caught-up·quorum을 뜻하지 않으며, 미확인 종료는 계속 `UNKNOWN`이다. 별도 `stop` 명령은 없고 실행 owner가 정확한 process에 SIGTERM을 요청한 뒤 process 종료, 같은 attempt의 완전한 `STOPPED / STORAGE_AND_CONSENSUS_CLOSED`, 두 closure field를 함께 확인해야 한다.

[Clean lock 예시](../../../packaging/engine-lock.clean-development.example.json)는 이번 JAR/전체 engine-info identity를 고정한다. `javaSha256`은 0으로 채운 미선정 값이므로 그대로 실행하는 제품 lock이 아니다. 검토해 선택한 Java 실행파일 hash와 실제 제품 runtime inventory는 별도로 기록한다. 기존 예시는 수정하지 않았다.

## 남은 준비

실행·상태 계약은 제공돼 init/lifecycle adapter 개발을 시작할 수 있다. native node JSON과 BXDL instance JSON의 연결을 검증하고, 신규 data·중단 init·기존 run을 구분하며, attempt/report 기록과 partial·unknown 실패를 보존해야 한다. cold wrapper의 강제 timeout 종료를 data 변경 명령에 그대로 적용하지 않는다.

공급 묶음에는 source-free QBFT provisioning/거래 generator, JRE, PKI·운영 설정이 없다. 공급 당시에는 준비 담당이 미정이었으며 이번 BXDL 계획에서는 source-free QBFT provisioning·거래 fixture 준비를 제품 후속 책임으로 기록한다. G1-M 전에 공개 chain/validator membership, node 설정, validator keystore/password와 mTLS trust/pin 발급·배치, 거래 제출·동일 높이 비교 fixture의 정확한 입력을 마련해야 한다. 이 결정은 NIGO의 canonical key/chain·합의 검증 책임이나 요구 원장을 변경하지 않으며, 준비 기능이 구현됐다는 뜻도 아니다. INSTANT smoke 예시나 기존 Gradle/npm devnet generator를 source-free QBFT 제품 인수로 표시하지 않는다.

선정 JRE·전체 Mac package·launchd·오프라인·4-validator 소비자 인수, Linux/systemd·multi-host, NIGO-05/A7, power-cut·장기 운영, 정식 release·LICENSE/NOTICE/SBOM은 별도다. 이번 수신으로 REQ-0002 전체의 `OPEN`을 변경하지 않는다.
