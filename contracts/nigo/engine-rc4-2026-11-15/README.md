# NIGO engine RC.4 수신 — 2026-11-15

**RC4-01 수신·전체 identity·선정 JRE의 실제 `engine-info` PASS. RC.4 lock 선택·runtime 조립·제품 인수는 미실행이다.**
기존 RC.1/RC.2/RC.3 snapshot·lock/spec·결과와 설치본은 유지했다. 다음은 [RC4-02 계약·fixture 영향 확정](../../../design/2026-11-15-rc4-adoption-plan.md)이다.

## 직접 수신한 후보

| 항목 | 값 |
| --- | --- |
| 인계 문서 revision | `7d15cd394ca9f75a8ab7eee35c0f8a7bae3b0977` |
| tag / source | `nigo-engine-v0.1.0-rc.4` / `037c98c315d5322fa7da266ca0254d61f3863622`, dirty=false |
| source tree / annotated tag | `335859ac6ddbf4cb05cfa47c0b53a164bb36fabb` / `2f0a0ca513e565ad59ecb660cc53961df5d11a32` |
| JAR SHA-256 / bytes | `56887aa12f2386f7211b11d56506aa42ab392c09f46c40f1431893ce77f6fa0a` / 164,133,636 |
| archive SHA-256 / bytes | `2c41581ff531e04a0302bab9cec268628929430858e1e8eba324ae1b75edd24e` / 157,700,874 |
| 원본 manifest SHA-256 | `a3af07fcf1a933123ede138a6e5d27150469bce8040d0fc77340a38b86bb28c6` |
| 계약 fingerprint | `c604b1d24097015b2839d97f2bc9d45e3fff91ed6984665d1aa827a155c7b6e8` |
| 실제 Console fingerprint | `a7c768df5d1ad8a09060cbab33b6fd3f0e3475d4ee0f90d7c76c445433ab33b6` |
| runtime | 기존 선정 Temurin21.0.12.1+1-LTS full JRE / macOS arm64·259파일·49모듈 |
| Java launcher SHA-256 | `9be1d0a740ff6502df1a762145e62860f5de4b7e17658d9cb9498da3acf9d16c` |
| runtime lock 원문 SHA-256 | `fb779d849cdb9153b694b3ad368dc64b178af71a118d148dcd4d51a3e7853761` |

[고정 인계](https://github.com/bwg-loonshots/nigo-protocol/blob/7d15cd394ca9f75a8ab7eee35c0f8a7bae3b0977/requirements/REQ-0003-engine-rc4-handoff.md)와
[private prerelease](https://github.com/bwg-loonshots/nigo-protocol/releases/tag/nigo-engine-v0.1.0-rc.4)를 직접 확인하고 새 private 경로로 다운로드했다.
release ID445809685, archive/checksum asset ID749821483/749821484의 서버 digest·크기와 받은 bytes를 대조했다.
원격 tag의 source/tree도 일치했다. 서버 보호·태그 검증의 실제 조회값은 [소비자 수신 기록](./consumer-receipt.json)에 기록한다.
checksum·서버 digest의 일치를 공급자 서명 인증으로 표현하지 않는다.

## 검증 범위

- archive·checksum asset의 신뢰한 SHA/크기·sidecar·gzip을 검사한 뒤, 고정 digest 아래 일반 파일24개의 안전한 상대경로·중복/link/special file 부재를 확인하고 수동 추출했다. SHA256SUMS 외23파일 전체 membership과 hash가 일치했다.
- 원본 JAR/manifest·embedded build/Console·공급 engine-info 전체 identity가 일치했다. 계약4개의 고정 source/archive/JAR 원문과 fingerprint, 운영 문서3개를 대조했다. 고정 Console source fingerprint를 재계산하고 JAR Console 전체 asset inventory를 대조했다.
- 공급자의 정제 요약 수치·증거 pin 상호 일치를 확인했다. source 집계는 제공된 aggregate이며 원자료 독립 감사나 BXDL의 해당 시험 실행이 아니다. source·mock UI·exact-JAR·선정 JRE 결과는 별도 범위로 기록했다.
- 새 private cwd/home/tmp와 clean environment에서 받은 JAR의 engine-info1회를 실행했다. engine-info는 exit0·stderr0·엄격한 단일 JSON이며 전체 identity가 공급 원문과 일치했다. 최종 검증의 속성·module 조회2회와 앞선 검증기 파싱 실패 때의 속성 조회1회를 포함해 JVM 호출은 총4회(속성2·module1·engine-info1)다. 자동 재시도는0회다.
- 호출별 대기 timeout은30초이며 종료 정리 유예는 별도다. 실제 호출은 정상 종료했고 호출한 process group의 부재를 확인했다. JRE259파일과 공급24파일의 membership/hash/size/mode는 실행 전후 동일했다.

[실제 engine-info](./consumer-evidence/engine-info.json)와 [실행 범위 기록](./consumer-evidence/engine-info-verification.json)이 소비자 실행 근거다.
private 검증기·원문 출력·전후 inventory·원격 API 응답은 별도 보관하고 이 snapshot에는 digest와 정제 판정을 남긴다.

엔진 호출 전 임시 검증기 오류2건을 정정했다. 첫째는 동일259파일을 서로 다른 정렬 순서로 비교한 오류로 JVM 실행 전이었다.
둘째는 정상 종료한 Java 속성 출력의 빈 값을 파싱하는 오류로 engine-info 실행 전이었다. 최초 검증기·실패 이력과 첫 속성 조회 출력·전후 inventory를 보존했다.
JRE/JAR 불일치나 엔진 후보 실패로 분류하지 않으며 최종 성공 결과와 구분한다.

## 원문 보존과 남은 단계

공급 원문22개는 bytes 그대로다. README/SHA256SUMS는 supplier- 접두만 붙이고 계약·운영 문서 위치만 평탄화했다.
원문 상대 링크는 NIGO/공급 bundle 기준이므로 이 snapshot에서 링크를 고쳐 원문을 변경하지 않는다.
supplier-SHA256SUMS는 원본23 payload의 목록이며 이 snapshot용이 아니다. 별도 SHA256SUMS는 자신을 제외한26파일을 검증한다.
JAR·JRE·archive·키·DB·미정제 로그는 Git에 넣지 않는다.

원본 manifest의 NOT_RECORDED/NOT_RUN/NOT_CHECKED/PROPOSED와 포장 시점의 SUPPLIER_VALIDATED_LOCAL_BUNDLE·NOT_PERFORMED_BY_PACKAGING_SCRIPT를 보존했다.
최종 NIGO DELIVERED와 BXDL 수신 PASS는 별도 시점·범위의 기록이며 원문 상태를 소급 변경하지 않는다.

수신으로 RC.4를 제품 선택하거나 기존 패키지를 교체하지 않았다. init/run/storage-integrity/generator·서비스·DB·네트워크 시험은 실행하지 않았다.
후속 RC.4 인수는 새 DB·새 genesis의 동종 개발망으로 수행하며 RC.3 DB migration·혼합 버전 rolling을 지원하지 않는다.
기존 계약 fingerprint 일치만으로 DB·Native catalog·wire 호환을 인정하지 않는다. helper 실물 사용 여부는 RC4-02에서 확인한다.
CLI05-04 사람 과업, A7, 전체 G1-M·J5·REM-01·기존 설치 upgrade·Linux/systemd·Docker·장기/운영 인수는 별도다.
development·officialRelease=false·PROPOSED, NIGO 자체 조건 UNDECIDED·고객 납품 NOT_GRANTED와 REQ-0002/0003 OPEN을 유지한다.
