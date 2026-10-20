# 네트워크 설정 준비와 RC1 cold 검증

2026-10-19 · BXDL `cd15569` 이후 변경 · [정제 JSON](./2026-10-19-network-preparation.json).

## 구현과 자동 검사

`config fingerprint`는 공개 leaf 인증서의 NIGO MTLS pin을 계산한다. `setup --network`는 기존 제품 설정·승인 chain·전체 validator 공개 목록에서 RC1 native를 준비한다. 정책별 문답, 취소/재개, 공개 입력의 파일 identity/hash 고정, 새0600 native 저장을 지원하며 기존 제품 v1·설치 journal은 유지한다.

최종 `make check`는 **343 PASS / 0 failed / 0 ignored**다. fmt·Clippy·Mac release 빌드·Linux 타입 검사를 통과했다. Linux 실행 인수는 아니다. 첫 검사에서는 새 통합 테스트가 사람용 출력에서 JSON 상태 상수를 찾는 assertion 하나가 실패했다. 실제 사람용 미실행 안내를 확인하도록 고친 뒤 최종 전체 검사를 통과했다.

독립 검토에서 network/chain을 읽기 전 민감 경로 겹침 검사, 게시 I/O·후속 검사 실패의 불명 결과 보존을 보완했다. 내용 읽기 관측 및 게시 실패 회귀를 추가했고 재검토에서 남은 명확한 주요 문제는 없었다. 게시 불명은 `UNKNOWN`/exit6이며 이후 `save`로 성공 처리되지 않는다.

## 실제 Mac 시험

새 release CLI와 기존에 검증한 설치 package의 고정 RC1·Temurin runtime을 사용했다. 실행 중 NIGO source·Gradle·인터넷을 사용하지 않았다. 기존 시험 자료는 복사해 새 경로에 배치했고 기존 데이터나 서비스는 변경하지 않았다.

1. 실제 공개 leaf DER의 NIGO pin을 CLI로 계산하고 하네스의 독립 domain-separated SHA256과 비교했다.
2. 승인된 시험 chain의 validator 4개를 포함한 공개 목록을 준비해 로컬 노드에 peer 3개가 렌더되는지 확인했다.
3. 실제 TTY에서 TLS 형식 선택 후 취소, 같은 세션 재개, new-network·합의 자동 시작 no·archive yes 선택과 native 저장을 확인했다.
4. 같은 세션에서 join-network·합의 자동 시작 yes로 바꾸고 다른 새 native 파일을 저장했다.
5. 두 출력 모두 제품/native binding `MATCHED`, 정상 cold `INCOMPLETE`/exit5를 반환했다. 생성 파일은 0600이며 DB/data 폴더는 생성되지 않았다.
6. 엔진 JAR와 java launcher hash 및 runtime 전체259파일의 경로·크기·mode·hash를 고정한 provenance와 대조했다.

**이 설정 준비·cold 범위는 PASS**다. 로컬 signer/TLS는 기존 시험 자료이며, 나머지 3개 peer의 node ID와 pin은 구조 검증용 합성 값이다. 실제 peer 인증·통신·4-validator 거래/합의·catch-up을 시험한 것이 아니다. 새 CLI를 담은 archive 설치·등록·기동도 이번 시험에서는 수행하지 않았다. 두 cold의 INCOMPLETE를 runtime READY로 바꾸지 않는다.

## 고정한 bytes와 후속

| 항목 | 값 |
| --- | --- |
| BXDL source digest | 3006375d44db6a2758bf8a3adc17b9b436b21be7104f080c94c1dd558876c450 |
| CLI SHA256 | 120089de764575709fe5d708a2d502df7dafd0b659ec4086c1176bfb8c0a803d |
| engine source | 540a89f0814c01d2a4ec67da1087af35195882ca |
| engine JAR SHA256 | f1ea80e25052221ed8a6e3108e020cb1b776833f4f86a29aae0885ad41703f45 |
| Java SHA256 | 9be1d0a740ff6502df1a762145e62860f5de4b7e17658d9cb9498da3acf9d16c |
| 환경 | macOS 26.6.2 (25G83), arm64 |

하네스·전체 검사·원자료 hash는 정제 JSON에 기록했다. 실행 자료는 로컬 시험 폴더 `/private/tmp/bxdl-network-preparation-4z9e6ska`에 보존하고 실제 키·native 원문·원본 로그는 Git에 포함하지 않는다.

다음 작업은 이 UX를 사용하는 동일 실제 package의 4-validator 인수다. 각 노드의 실제 NGVK·개별 TLS/신뢰·NIGO pin과 거래 fixture를 준비해 거래 확정·동일 finalized 높이/hash/root·재시작/catch-up·잘못된 인증 거부를 확인한다. 안전한 제거·Mac 운영 실패/로그아웃/sleep·J5·전체 G1-M은 별도 잔여다.
