# RC.4 계약·fixture 영향과 유한 제품 인수 범위

- 작성: 2026-11-15. BXDL 기준 `66c928936b54acd74bf3bab9acb033b44e0e7729`. RC4-02의 정적 영향 검토 문서다.
- 기준: RC.3 source `7ec5abd8c98e1cf070763a16f815e9281b74a5c1`와 RC.4 source `037c98c315d5322fa7da266ca0254d61f3863622`의 직접 tree 차이, [RC4-01 수신본](../contracts/nigo/engine-rc4-2026-11-15/README.md).
- 범위: 고정 source와 수신 JAR의 ZIP entry·원문을 읽었다. 이 문서의 정적 확인은 helper 실행, genesis DB 생성, 제품 선택, Mac 서비스·네트워크 인수가 아니다. fixture 준비 실행 여부는 별도 결과에 기록한다.
- RC.3 lock/spec/runtime/profile·기존 결과를 유지한다. RC.4 실행 profile과 새 runtime 조립은 RC4-03/04다.

## 1. 관리 계약과 소비자 DTO

엔진 계약4파일 `ENGINE_CONTRACT.md`, `src/main/resources/engine/contract.json`, `ENGINE_RUNTIME_CONTRACT.md`, `src/test/resources/engine-contract/health-cases.json`은 두 고정 source에서 동일하다. fingerprint도 `c604b1d24097015b2839d97f2bc9d45e3fff91ed6984665d1aa827a155c7b6e8`로 같다. `nigo-node`의 `engine` 제품 소스와 관리 bootstrap·health controller/DTO/service에도 직접 차이가 없다. 계약 revision은 RC.4 source로 바뀌며 실제 engine-info의 source·Console identity는 RC4-01에서 따로 대조했다.

BXDL은 `src/engine/runtime_http.rs`에서 `/monitor/api/console/bootstrap`과 `/monitor/api/consensus/health`만 호출하고, `runtime_result.rs`에서 해당 전체 JSON과 init/run 보고서·identity를 엄격히 검증한다. 이번 정적 비교를 근거로 parser 허용 범위를 넓히거나 unknown field 검사를 제거하지 않는다. 실제 RC.4 응답은 RC4-05에서 기존 parser로 확인한다. 외부 CLI의 RC.3 고정 source/JAR 응답 검증을 RC.4 profile로 확장하는 작업은 별도 RC4-04다.

추가 관측 DTO는 `HistoricalSyncServingStatusDto.capture`다. 값은 초기 미연결 시 null일 수 있으며 연결 뒤 capture/retry 간격, finalized/archived/lag 높이, 마지막 시도·성공·실패 높이, 시도·성공·skip·연속 실패 수, 마지막 시간·bytes·cells 및 skip/failure 이유를 제공한다. 이 historical serving DTO는 현재 BXDL 두 endpoint parser의 직접 입력이 아니다. 향후 별도 진단에 도입할 때 nullable 값·단위·증거 범위를 정의한다. capture 지표 자체를 quorum·성능 SLA 판정으로 사용하지 않는다.

RC.4 내부에는 archive capture 간격·재시도 설정이 추가됐지만 기본은1block/8blocks다. 관리 CLI의 승인된 native 설정 whitelist가 확대된 것은 아니다. RC4-02에서 임의 Spring property나 새 UX 항목으로 노출하지 않는다.

## 2. Native catalog와 genesis 경계

표준 공급 catalog는 동일한 chainId11578·schema·revision 체계를 유지하면서 실제 runtime/descriptor/composition bytes와 공급 pin이 바뀌었다.

| 항목 | RC.3 | RC.4 |
| --- | --- | --- |
| `standard-product-candidate/lock.json` SHA-256 | `48790ff2e52e06ecb360efe0ad9a29a53ac087ea6f4ed97d07ef6271f2efba27` | `8aa6912d714cc829f6a6421ff3ee032e2de544c53a349801dcaae6812cb9d25b` |
| lock bytes / payload 파일 수 | 1,000,681 / 378 | 1,000,681 / 378 |
| payload 전체 bytes | 9,026,694 | 9,017,808 |
| stock root / dependency | 21 / 26 | 21 / 26 |
| NIGO root / dependency | 1 / 1 | 1 / 1 |

수신 JAR의 `BOOT-INF/lib/nigo-vm-0.0.1-SNAPSHOT.jar` 안 `native/standard-product-candidate/lock.json`과 `lock.sha256`을 확인했다. 해당 prefix의 regular entry는 lock·sidecar를 포함해380개다. lock이 지정한378 payload의 membership·SHA-256·크기와 합계9,017,808bytes를 정적으로 대조했다. 이는 classpath 공급 inventory 일치이며 Java catalog loader의 의미 검증이나 실제 genesis 초기화를 실행한 결과가 아니다.

RC.4 lock은 `schema=nigo-standard-product-candidate-v1`, `chainId=11578`, `acceptance=LOCAL_DEFAULT_SUPPLY`, `defaultActivated=true`이며 artifact49개 중 stock47개와 NIGO2개다. 모든 파일·schema를 임의로 최신 공급물로 대체하지 않는다.

`NativeStandardProductGenesis`는 고정 classpath 공급을 선택하며 catalog 설정 옵션이나 legacy fallback을 제공하지 않는다. `NativePolicyGenesisAssembly.verifyState()`는 현재 공급으로 만든 기대 Cell과 기존 DB Cell을 exact 비교한다. 따라서 RC.3 표준 catalog DB를 새 JAR로 여는 것은 지원하지 않으며, 거절될 것이라는 판단도 소스 대조에 따른 예상이지 실제 구 DB 교체 시험 결과가 아니다.

현재 `chain_properties()`에는 stock catalog 선택 값이 없다. RC.4 JAR의 고정 기본 공급을 사용하고 새 chain 입력·자격증명을 준비한다. **RC4-02는 fresh genesis를 위한 init 입력 준비이며 genesis DB 생성은 아니다.** 실제 genesis는 RC4-05/06의 명시 `init`에서 새 data 경로에 생성한다. 기존 RC.3 DB·genesis 파일·서명 거래·키를 복사하지 않는다.

## 3. Console 및 변경된 엔진 동작

Console source의 직접 차이는 `package.json`, `package-lock.json`, `src/features/explorer/standardProductCatalog.json`이다. Vitest4.1.12 및 lockfile 변경과 표준 공급 projection이 반영돼 최종 Console fingerprint는 `a7c768df5d1ad8a09060cbab33b6fd3f0e3475d4ee0f90d7c76c445433ab33b6`다. 11/12 이전 검토의 추정 fingerprint로 고정하지 않는다.

Console catalog의 top-level 의미는 유지되고 `sourceLockSha256`이 새 Native lock으로 바뀐다. stock47개(21root/26dependency) 구조는 유지되지만38개 artifact 내용이 달라졌다. 실제 엔진이 제공하는 catalog와 묶인 Console projection의 일치를 RC4-06에서 확인한다. Console 자체의 기능 확장이나 중단한 `bxdl console` 개발 재개는 포함하지 않는다.

| 엔진 변화 | BXDL 영향·확인 경계 |
| --- | --- |
| BLOCK_SYNC의 `LIVE_NEXT`/`LIVE_DRAIN`, bounded live recovery | 같은 RC.4 binary로만 망 구성. 기존 wire revision 값만으로 mixed-version 호환을 판단하지 않음 |
| snapshot/historical proof·retention·RocksDB WAL | 새 DB 초기화와 정상 종료·같은 RC.4 DB 재시작·catch-up을 확인. 과거 receipt 전체 보존, power-cut, hard disk quota, SLA로 확대하지 않음 |
| Type-4 `eth_call`·`eth_estimateGas` | authorizationList 추론·sender nonce 순서·격리 simulation·gas 의미 변경. 기존 Type-2 송금 fixture 통과만으로 Type-4를 검증했다고 표시하지 않음 |
| KZG provider 장애 전파·BLS와 공개 `0x0106`/내부 READ·PLAN 경계 | 공급자의 exact-JAR 증거와 제품 package native/API 통합 검사를 구분. fault 주입·별도 엔진 재빌드·임의 JAR patch는 기본 제품 재인수에 포함하지 않음 |

## 4. 시험 전용 helper와 새 준비 자료

`QbftDevnetGenerator.java`와 `devnet/qbft` 소스는 RC.3→RC.4 직접 diff가 없다. 수신 JAR에 generator 본체·Options·Manifest/ManifestNode·MTLS helper 및 Spring `PropertiesLauncher` class가 포함됨을 정적으로 확인했다. 이 사실만으로 실행 성공 또는 공식 source-free 생성기·고객 PKI 제품 공급을 선언하지 않는다.

기존 준비 호출 계약을 유지한다.

- 명시 RC.4 JAR/hash/size와 선정 full JRE를 사용한다. Java launcher·keytool pin 및 helper entry를 실행 전에 대조한다.
- `PropertiesLauncher`의 `loader.main=org.nigo.node.devnet.qbft.QbftDevnetGenerator`로 새 private 출력 경로에 validator4/observer0, MTLS, chainId11578, profile `DEV_QBFT`, 분리된 HTTP/P2P 포트, request timeout5000/max20000을 전달한다. certificate-lifecycle은 새로 활성화하지 않는다.
- helper 결과 `devnet.json`의 chain/profile/security, validator4개의 index/role·경로·nodeId·validatorId·credentialFingerprint·포트를 검사한다. 각 validator의 keystore/password와 peer identity/trust material6파일은 regular0600이어야 한다.
- 생성한 `application.yml`은 원자료다. BXDL 제품 입력은 승인된 whitelist의 `chain.json`, `network.json`, `products/n1..n4.json`으로 별도 만든다. 생성 YAML의 H2 예제를 제품 backend로 가져오지 않고 RocksDB를 유지한다.
- 새 offline wallet과 Type-2 거래12개를 준비한다. nonce/hash/chainId/type·서명 주체를 자체 대조하고 제출은 하지 않는다. Type-4/KZG 전용 입력이 추가되면 별도 case·input digest로 기록한다.
- 준비 완료 기록은 `PREPARED_NOT_EXECUTED`, `nodeDataCreated=false`, `nodeStarted=false`, `networkTraffic=NOT_PERFORMED`, `runtimeAcceptance=NOT_CHECKED`를 유지한다. 같은 helper/JRE/JAR·입력·harness identity를 실행 전후 대조한다.

준비 후보 whitelist와 제품 실행 후보 whitelist를 분리한다. RC.4 fixture를 만들 수 있어도 아직 RC.4 runtime/profile을 실행할 수 있다는 뜻이 아니다. 실제 수신 JAR로 준비 helper를 사용할 수 없으면 원인·identity·입력을 보존하고 NIGO에 지원을 요청한다. source 재빌드나 RC.3 자료 재사용으로 우회하지 않는다.

## 5. RC4-05/06의 유한 통합 목록

아래는 새 runtime R·외부 관리 CLI B가 고정된 뒤 수행할 목록이다. RC4-02 준비 성공을 이 제품 사례의 PASS로 승계하지 않는다. 실행별1회·자동 재시도0을 기본으로 하며 입력/횟수/timeout/정리와 PASS·FAIL·NOT_RUN을 기록한다.

| 단계·사례 | 제품 완료 근거 |
| --- | --- |
| RC4-05 설치 준비·apply | fresh 입력의 순차 문답·저장/취소/재개/한 항목 수정·조회 무쓰기·설치 원본 경로 부재에서도 보관 자료로 연결 |
| RC4-05 init·보고서 | 명시 init으로 새 DB 생성, 실제 RC.4 source/JAR/catalog identity, 보고서0600과 logs/diagnose의 완전 수집. 기존0644 복구는 별도 |
| RC4-05 서비스·같은 DB 재시작 | bootstrap/health를 기존 엄격 parser로 소비, 상태/이름 기반 start·stop·동일 RC.4 DB restart, worker/JRE/payload 불변·정상 정리 |
| RC4-06 동종 4validator·거래·catch-up | 4/4·3/4 확정과 뒤처진 노드의 header/state catch-up. snapshot catch-up의 과거 receipt 가용성은 성공 조건으로 추가하지 않음 |
| RC4-06 무정족수·live 복구 | 2/4 유한 관측 후 복원. 원 거래 hash로 관찰하며 자동 재제출 없이 확정 확인 |
| RC4-06 같은 DB round-skew 복구 | 별도 유한 시나리오에서 명시된 거래만1회 재제출. live 무재제출 경로와 분리 |
| RC4-06 잘못된 peer pin | fresh 별도 negative 입력으로 승인되지 않은 mTLS peer 거부와 미확정 관찰. 정규 시험 자격증명은 변경하지 않음 |
| RC4-06 Native·Console 정합 | 실제 새 genesis와 제공 catalog/Console의47stock projection·새 lock identity를 대조. helper/ZIP 정적검사와 구분 |
| RC4-06 API/native 변경 영향 | 공급자의 여섯 exact-JAR runner pin을 연결하고 같은 R+B의 Type-4 call/estimate·KZG 정상/잘못된 proof·공개 host 검사를 준비 가능한 유한 입력으로 선정. 실행하지 않은 세부 의미는 NOT_RUN으로 남기며 기존 Type-2 suite의 PASS로 대체하지 않음 |
| RC4-06 관리 CLI update/rollback | 동일 RC.4 profile을 지원하는 두 외부 CLI를 실행 중 전환/복귀. R/A/JRE/JAR·노드 data 불변, engine/DB rollback과 구분 |

API/native 행은 RC4-04 하네스 설계 때 각 입력·기대값·호출 수를 확정해야 실행 가능하다. NIGO 원자료의 source 시험·fault 주입·mock UI를 그대로 실행하는 무제한 묶음으로 만들지 않는다. 관측 추가가 필요하면 진단 전용으로 구현하고 제품의 기존 bootstrap/health 검증을 약화하지 않는다.

모든 서비스 시험 뒤 소유 PID·LaunchAgent·TCP listener 부재, 입력·package inventory 불변을 독립 대조한다. Linux/H2 제품 지원·RC.3 DB migration·혼합 binary·장기 soak·전원 차단·성능 MAIN·고객 배포는 별도다.
