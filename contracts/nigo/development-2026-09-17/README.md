# NIGO 개발 후보 계약 수신 — 2026-09-17

이 디렉터리는 NIGO 제공자 원문 4개와 실제 개발 후보 manifest의 **byte 단위 스냅샷**이다. 원문을 편집하거나 BXDL의 의미를 덧붙이지 않았다. 원문 안의 상대 링크는 NIGO 저장소 기준이며, BXDL의 경로로 재작성하지 않았다. 아래 설명만 BXDL이 작성했다.

상태는 `PROPOSED`, channel은 `development`, `officialRelease=false`다. 이 스냅샷의 존재는 양측 상세 계약 확정, 실행 승인, 신뢰 root, 정식 공급 또는 BXDL 제품 인수를 뜻하지 않는다. 제품 코드는 이 문서만 보고 임의 manifest/JAR를 승인해서는 안 된다. 별도로 선택한 공급 경로·예상 hash·lock과 실제 bytes, 계약 identity를 검증해야 한다.

## 수신 identity와 출처

| 항목 | 확인한 값 |
| --- | --- |
| 읽은 NIGO checkout | `17c0bc3b63756915c18fa2942afdd73426bb2eac` — PR #117 병합 |
| JAR에 기록된 source commit | `aee1cbe5e0383ea9eb4d3334d72e3ea23d1d8b27` |
| source dirty | `true` — 위 commit의 clean 산출물 또는 전체 작업 트리 attestation이 아님 |
| contract revision | `aee1cbe5e0383ea9eb4d3334d72e3ea23d1d8b27` |
| contract fingerprint | `0ac719e0175e9d5fb2e732e297b496e5725bd17f18d84bb53c6b51c235fd71cc` |
| console fingerprint | `aa813cd89e0455d3e049075d31fa74bf86e94a3e3d5b8fb6c4190a52e393f8d1` |
| JAR 파일명·크기 | `nigo-node-0.0.1-SNAPSHOT.jar`, 159,965,726 bytes |
| JAR SHA-256 | `37070fbddaf1350b81952b9f3bd718b76f246557d7cd5abe2ec580a13d8de5fe` |
| Manifest 원문 SHA-256 | `74c8a312a075e496befc62f14f6f700b89a6a8845eaee61d872df7fa144ea00d` |
| Java 요구 | major 21; 선정 BXDL JRE 공급자·patch·전체 runtime 인수는 별도 |
| 실제 원본 위치 | NIGO `nigo-java/nigo-node/build/distributions/engine-development/` |

읽은 HEAD와 JAR source commit은 서로 다르다. NIGO 작업 기록은 이 JAR 생성 후 IDE 경고 수정에 대해 일부 source 테스트만 다시 수행했고 JAR 전체를 재생성하지 않았다고 명시한다. 따라서 HEAD의 모든 구현이 이 JAR에 포함됐다고 표시하지 않는다. JAR 자체는 이 저장소에 복사하지 않았다.

## 원문·JAR entry 교차 확인

다음 순서는 공급자 `contractInputs`의 순서다. 최초 채취 시 각 snapshot 원문 bytes가 NIGO source bytes 및 해당 JAR entry bytes와 같은 것을 확인했고, 각 hash와 aggregate fingerprint도 manifest와 대조했다.

| 스냅샷 | NIGO sourcePath | JAR entry | SHA-256 |
| --- | --- | --- | --- |
| [ENGINE_CONTRACT.md](./ENGINE_CONTRACT.md) | `nigo-java/nigo-node/ENGINE_CONTRACT.md` | `BOOT-INF/classes/engine/contract-evidence/ENGINE_CONTRACT.md` | `c39b424aff032d219713174f5f84862cf6719502d99174f8f5d46eb2073d9534` |
| [contract.json](./contract.json) | `nigo-java/nigo-node/src/main/resources/engine/contract.json` | `BOOT-INF/classes/engine/contract.json` | `6816bd9a45d4b4f739aad902946fd29f77f420ef79e70e20a3200ad3ba5ce451` |
| [ENGINE_RUNTIME_CONTRACT.md](./ENGINE_RUNTIME_CONTRACT.md) | `nigo-java/nigo-node/ENGINE_RUNTIME_CONTRACT.md` | `BOOT-INF/classes/engine/contract-evidence/ENGINE_RUNTIME_CONTRACT.md` | `7665c195987708143fce10e9a1eed609a8367068f8bacfdd7e89cb14d14b3383` |
| [health-cases.json](./health-cases.json) | `nigo-java/nigo-node/src/test/resources/engine-contract/health-cases.json` | `BOOT-INF/classes/engine/contract-evidence/health-cases.json` | `850105faa65199044e845e648f3c3dbb1e3833001b0eec825ac8ecdd36e3f132` |

Aggregate는 이 순서대로 `UTF-8(sourcePath) + 0x00 + 원문 bytes + 0x00`을 연결한 SHA-256이다. 실제 JAR 크기·SHA-256이 manifest와 일치하고, 내장 `BOOT-INF/classes/engine-build.json`의 모든 field가 manifest의 대응 subset과 일치하는 것도 확인했다. 이 비교는 읽기 전용 ZIP/hash 검사이며 엔진 실행은 아니다.

같은 날 최종 재검사에서 NIGO HEAD는 여전히 `17c0bc3b…`였으나 source 계약 4개에 미커밋 변경이 생긴 것을 관측했다. 현재 source의 SHA-256은 위 순서대로 `b9d4f0f77344b6cb8db22586b762f4c2e5713b847a2199421efed4cdb0ad873d`, `bffe91e25a3c58db7626fadbb8333b5216ab293b7760dee033f2fdf974841687`, `ec52f844e9291f6a3cd17a4d93e58a71c2acd651e1706c9c7f9b53b6842cf43e`, `c64d4003e8b7dba63845f17dfe1386546988640e1af61fba478206c83402c01e`였다. 이 스냅샷은 계속 위 후보 JAR 내부 entry 및 manifest와 일치하며, 새 source 문서로 교체하지 않았다. 후속 작업 트리의 변경을 이 후보의 계약으로 소급 적용하지 않는다.

[engine-manifest.json](./engine-manifest.json)은 위 실제 distribution의 원문이다. `evidence.supplierTests=NOT_RECORDED`, `consumerAcceptance=NOT_RUN`, `native.validationStatus=NOT_CHECKED`, 빈 `testedPlatforms`를 그대로 보존했다. 공급자 작업 기록 `tasks/2026-09-17-bxdl-engine-foundation.md`에는 위 JAR hash에 연결된 exact-artifact 명령 시험 8개 통과 등 별도 근거가 있다. manifest가 시험 결과를 자동 첨부하지 않는다는 설명과 실제 소비자 시험을 구분한다. 이 스냅샷을 만들며 NIGO를 빌드하거나 노드를 실행하지 않았다.

## BXDL이 소비할 때의 주의점

- `engine-info`는 JSON metadata object와 exit 0을 반환한다. exit 0만 확인하지 말고 `buildInfoStatus=AVAILABLE`과 외부 pin/manifest의 identity 일치를 요구한다. `MISSING`/`INVALID`도 exit 0일 수 있다.
- `preflight --config=/absolute/node.json`은 정상 정적 검사에도 exit 3, `INCOMPLETE/RUNTIME_CHECKS_REQUIRED`다. DB/WAL, network, native/runtime은 `NOT_CHECKED`로 남긴다. NIGO exit 3을 BXDL artifact 실패 exit 3과 혼동하지 않는다.
- NIGO node JSON은 `chainFile`, `dataDirectory`, `backend`, `node`이며 BXDL instance JSON과 다르다. 제품 설정만으로 validator identity·peer·mTLS pin·sync 구성을 추측하지 않는다. 호출은 고정 인자 배열을 사용하고 임의 JVM/환경 설정을 전달하지 않는다.
- preflight 결과의 `chainFingerprint`는 raw chain 파일 hash가 아니라 정렬된 chain property 문자열 map의 JSON hash다. 결과에는 별도 build identity·attempt ID가 없으므로 호출한 exact JAR/입력과 연결해 소비한다.
- runtime `READY`는 local 상태다. `syncStatus=UNKNOWN`, observer/manual-start의 `running=false`, progress `IDLE/NOT_APPLICABLE`, 큰 정수의 decimal-string wire 의미를 보존한다.

## 남은 공급자·제품 인수 조건

QBFT 종료는 내부 follower/server/signer 작업 전체의 정리가 증명되지 않아 `UNKNOWN/QBFT_CLEANUP_NOT_FULLY_OBSERVED`다. live follower의 source 단절 관측·자동 재선택, 전체 fresh `sync=true` bootstrap, genesis-only observer도 미완료 범위다. 재시작 공급자 시험은 observer의 source가 아닌 validator를 대상으로 했으며 source failover 성공이 아니다.

Clean commit 후보, 양측 계약 확정, 선정 JRE/native·Mac package/launchd·4-validator 소비자 인수, Linux/systemd·multi-host, NIGO-05 유지보수, 정식 release/SBOM은 별도다. 스냅샷을 근거로 REQ-0002 전체를 `DELIVERED`/`VERIFIED`로 바꾸지 않는다. BXDL의 실제 실행 결과는 [구현 상태](../../../docs/implementation-status.md)와 연결된 별도 results에 기록하며 이 원문을 사후 수정해 통과 기록으로 만들지 않는다.
